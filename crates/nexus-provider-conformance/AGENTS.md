# nexus-provider-conformance — Provider Stream Conformance Runner

Neutral cross-adapter conformance gate for normalized provider `HostEvent` streams
(v1.180 P0, RN-OGA-1). A PR that drifts native-provider stream lifecycle
(`claude-codes` / `codex-codes` / `dsh-native`) fails in CI without credentials.

## Purpose

This crate consumes the normalized-stream contract types
`nexus_agent_host::capability::model::{HostEvent, HostEventStream}` and asserts
adapter-normalized invariants over one operation stream, producing a typed
`ConformanceReport` of `ConformanceFinding`s. Task 2 drives the real adapters
through their public constructor-injected transport seam pointed at scripted
fixture CLIs; this crate stays provider-neutral.

## Key Rules

- **Adapter-normalized only**: the runner asserts invariants over normalized
  `HostEvent` streams. Vendor wire bytes live only in `fixtures/` (Task 2) —
  never parse vendor frames in this crate.
- **Neutral**: no provider-specific logic. The runner is a pure function of the
  normalized stream plus the `ConformanceConfig` bounds.
- **No new external dependencies**: path dep on `nexus-agent-host` only; reuse
  workspace-managed `futures-util` / `tokio`. Zero registry additions.
- **Typed findings**: every violation is a `ConformanceFinding` carrying an
  `InvariantId` and event-index evidence.
- **Drift gate**: closed-set checks (`error_category` five-token set,
  `FinishReason`, `SessionStopReason`, `StatusLevel`) fail on any value outside
  the contract set — a new vendor token or enum variant must trip conformance,
  never pass silently.
- **Not wired into product binaries**: this crate is a CI/test-only gate.

## Dependencies

- `nexus-agent-host` — normalized `HostEvent` / `HostEventStream` contract types

## Module Layout

- `lib` — `run_conformance` entry point
- `model` — `ConformanceReport`, `ConformanceFinding`, `InvariantId`, `ConformanceConfig`
- `invariants` — per-invariant checks: `started`, `bounds`, `ordering`, `terminal`, `stop_reason`, `values`

## Design Reference

See `.mstar/plans/2026-09-02-v1.180-p0-provider-conformance-runner.md` and
`.mstar/specs/agent-host.md` §3.4.
