# Spec: Daemon Restart reliability

**plan_id:** `2026-07-22-v1.130-p0-daemon-restart`  
**Status:** specify+clarify+plan locked (architect Seat 2)  
**Wave:** 1

## Problem

Status-footer Restart today lies and breaks flow. Three failure modes:
1. **False `port already in use`** after stop→start race — the daemon reports a conflict that does not exist, so Restart fails for no real reason.
2. **Delayed fullscreen recovery UX** — when Restart fails mid-session, the app forces a fullscreen recovery detour instead of an inline pending/fail state.
3. **Second Restart no-op** when the daemon is attached (`!owned`) — clicking Restart again does nothing because the app cannot restart a daemon it didn't start.

Author loses flow every time they touch Restart.

## Goals

- Atomic restart path (stop listening daemon + wait port free + start) for owned and attached
- Mid-session in-place pending/failure; no delayed fullscreen tax for footer Restart
- Honest zh-CN restart copy (not create-profile / CLI-only unless true external conflict)
- Regression tests: owned, attached, port-busy-other-process, second restart

## Non-Goals

- Changing default port; multi-instance product model; OS-keychain pinning (`R-V192SEC-001` deferred)

## Architecture decision (locked)

- One `restart_daemon` Tauri command invokes one serialized `SidecarManager` restart transaction. The web footer does not compose `stopDaemon()` and `startDaemon()`.
- Owned child: stop through the retained handle, wait for exit and free port, then spawn and health-check.
- Attached daemon: terminate only after successful Nexus health plus stable listener PID/process identity verification. A foreign or unverifiable listener is never killed and returns an honest conflict.
- The replacement child is desktop-owned. A second Restart therefore follows the same owned path and cannot become a no-op.
- A single-flight restart mutex and monitor stop flag prevent user restart, crash restart, quit, and repeated clicks from overlapping.
- Lifecycle events expose immediate pending and terminal running/error state; the footer owns inline UX while the launch gate remains cold-start-only.

## Interfaces

- Rust: `SidecarManager::restart_daemon`, existing stop/spawn/health helpers, one restart serialization primitive.
- Tauri: `restart_daemon` command.
- Web desktop boundary: `DesktopCapabilities.restartDaemon()`.
- UI: `apps/web/src/components/layout/daemon-status-bar.tsx` calls one method and renders pending/failure inline.
- Contract verdict: `wire_contracts_changed: false` (desktop IPC only).

## Acceptance

Per compass AC **Restart (P0)** section. Plan-level DoD maps T1–T4 → AC Restart.

## Risks

Port TIME_WAIT; monitor race during stop; attached PID discovery wrong process.
