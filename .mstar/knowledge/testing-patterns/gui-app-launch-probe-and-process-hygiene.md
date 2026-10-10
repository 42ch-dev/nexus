---
module: apps/desktop-electron (packaged app CI verification)
date: 2026-10-09
problem_type: testing_pattern
category: testing-patterns
severity: high
plan_id: 2026-10-09-v1.209-p2-desktop-launch-integrity
applies_when:
  - building a bounded CI probe that proves a GUI app actually launches
  - designing FAIL-on-early-exit semantics for consumer-facing build artifacts
  - retaining stderr for marker detection while bounding memory
  - cleaning up process groups whose descendants may outlive the leader
related_components:
  - apps/desktop-electron/scripts/verify-launch.mjs
  - apps/desktop-electron/tests/verify-launch.test.mjs
  - .github/workflows/desktop-electron-build.yml
tags:
  - launch-probe
  - process-group
  - sigkill
  - stderr-markers
  - test-fixtures
  - ci-gate
  - electron
---

# GUI-app launch probe: bounded, discriminating, and hang-free

## Context

Build pipelines prove a desktop artifact *builds*; they rarely prove it *launches*. The v0.1.0 darwin artifacts shipped while failing at dyld and, after that fix, at bootstrap (`product.json` resolution), and — once the icon file existed — at `dock.setIcon` with an undecodable `.icns`. Each defect was consumer-visible yet passed every construction-stage gate. A launch probe closes that class: a bounded direct-exec check that runs against the *produced* artifact and fails the job when the app cannot start.

`apps/desktop-electron/scripts/verify-launch.mjs` is the reference implementation; its contract and fixture design are the pattern below.

## Guidance

1. **Direct-exec the binary; never `open -W`.** LaunchServices accepting a request is not evidence of a living process — a probe run recorded `open` returning 0 while the app died 21 ms later. Probe `<app>/Contents/MacOS/<executable>` directly and observe the process.
2. **FAIL on any exit within the window.** A healthy desktop app does not self-exit during startup observation; do not pattern-match success on exit codes. The known bootstrap failure surfaces as exit 1 within ~10 s.
3. **Belt-and-suspenders stderr marker.** Independently of the exit path, FAIL when captured stderr contains the bootstrap failure marker (`[desktop] bootstrap failed`). This catches catch-and-continue failure shapes that would otherwise exit 0.
4. **Decouple marker detection from retention.** Keep only a bounded stderr tail (e.g. 64 KiB) for reporting, but detect markers on a streaming path that carries `marker.length - 1` characters across chunk boundaries and scans raw chunks. Trim-before-scan loses a marker straddling the retention cut and converts it into a false PASS on the window-timer path.
5. **Clean the process group on every exit path — awaited.** Spawn detached; on early exit, marker-fail, PASS-at-window-end, and error alike: SIGTERM the group, bounded-poll group liveness, SIGKILL survivors, then destroy the captured pipes — all *before* the probe resolves. Do **not** hand SIGKILL to an `unref()`ed timer: a standalone CLI can exit ahead of it, leaving SIGTERM-ignoring descendants alive and pipes held open.
6. **Fixtures must discriminate, not decorate.** Stub executables (`sleep`, early-exit, marker-exit, and a SIGTERM-ignoring descendant) live in fake `.app` trees; no real GUI is required. Two fixture rules earned the hard way:
   - A TERM-resistant descendant needs a **readiness handshake** (the descendant signals its handler is installed before the leader exits); otherwise the escalation race can make the fixture pass for the wrong reason.
   - Exit-window sizing must tolerate default-concurrency load: exit events were observed delayed up to ~756 ms under parallel test load, so a 500 ms window flaked 5/10 while a 5 s window stayed green 3×225/225. Assert outcomes that are timing-independent wherever possible (e.g. descendant reaped) instead of racing wall-clock edges.
7. **Ship the probe with red/green discrimination evidence.** Every new failure-detection fixture must fail against the unfixed revision and pass after the fix; record both. State the coverage boundary in the script header — the probe proves bootstrap completion and sustained liveness in a CI session, not Gatekeeper quarantine behavior or window rendering.

## Why This Matters

A launch probe converts "artifact builds" into "artifact is consumable". Because it runs on the produced/published artifact (not staged inputs), it also preserves the receipt's meaning: checks that validate staging can be true while the shipped bytes are broken (see `build-errors/packed-app-symlink-materialization.md`).

## When to Apply

- Any pipeline that publishes installable desktop bundles (CI step after the package/verify stage, on all matrix legs).
- Any bounded process-observer utility where descendants can outlive the leader (CLI wrappers, e2e harnesses, sandbox launchers).

## Examples

- Probe contracting + fixtures: `apps/desktop-electron/scripts/verify-launch.mjs`, `apps/desktop-electron/tests/verify-launch.test.mjs` (fixture names include the TERM-ignoring descendant and the straddling-marker cases).
- Pipeline wiring: `.github/workflows/desktop-electron-build.yml` (`Verify published app launches`, after `Verify published package`, before `Stage flat artifact root`, both legs, hard failure — no `continue-on-error`).
- Sibling verifiers: `verify-package.mjs` stays read-only and derives its executable name from `CFBundleExecutable`; the probe derives the same name so the two cannot drift.
