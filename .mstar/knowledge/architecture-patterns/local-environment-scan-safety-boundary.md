---
module: crates/nexus-acp-host + crates/nexus-daemon-runtime + apps/desktop + apps/web (setup-wizard)
date: 2026-07-06
problem_type: architecture-pattern
category: architecture-patterns
severity: high
plan_id: V1.94-P-last (compound of desktop onboarding & IA pass)
tags: [acp, agent-host, path-scan, security, execution-boundary, subprocess, bounded-concurrency, timeout, setup-wizard]
applies_when: scanning the local environment for installed tools/agents the daemon could talk to; any feature that probes PATH for binaries; any "auto-detect local X" UX where X is an executable
---

# Local-Environment Scan Safety Boundary (ACP Agent Detection)

**Track**: Knowledge (durable guidance distilled from V1.94 Desktop App Onboarding & IA Pass).

## Context

V1.94 introduced `POST /v1/daemon/agent-host/scan` — the daemon-side endpoint that powers the Setup Wizard's Step 3 "ACP Agent Detection". The endpoint combines:

1. The existing `RegistryClient` cache (`~/.nexus42/registry/cache.json`, fetched from `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`).
2. A new `scan_local_installations` helper in `crates/nexus-acp-host/src/registry.rs` that probes the user's PATH for each registry-known binary name and reports `installed: bool` + best-effort `version`.

The endpoint's output drives the wizard's default recommendation (first PATH-available agent) and lets the user pick. The user may also supply a custom `launch_command` after the scan, but the scan itself must not execute user input.

The challenge: **the daemon is executing subprocesses (`<binary> --version`) on behalf of the SPA**, mediated by an HTTP endpoint. The security boundary must be airtight — a malicious actor who can shape the binary name list (e.g. via a compromised registry cache) must not gain arbitrary command execution.

## Guidance (the pattern)

The `scan_local_installations` helper enforces **five normative constraints** (codified in `specs/desktop-shell.md` §14.3). All five are required; any one missing opens an attack surface.

| # | Constraint | Reason |
|---|------------|--------|
| 1 | **Registry-known binary names only** | The list of binaries to probe comes from the cached ACP registry, NOT from user input. A user-supplied "scan this command" string is forbidden during the scan phase; the wizard's custom-`launch_command` affordance comes AFTER the scan and writes to config, never to the scan itself. |
| 2 | **Bounded concurrency (≤4 simultaneous probes)** | PATH lookups + `--version` subprocess spawns are cheap individually but can fan out across the registry (dozens of entries). Unbounded concurrency can exhaust process/file-descriptor limits. A `Semaphore` (or `JoinSet` with a hard cap) enforces the bound. |
| 3 | **Hard ≤2s timeout per `--version` probe** | Some binaries hang on `--version` (broken stdio, blocking on stdin, network call). A hard per-probe timeout caps worst-case latency at `(registry_size / 4) * 2s` regardless of individual hangs. |
| 4 | **No shell expansion** | Use `tokio::process::Command::new(binary).arg("--version")` (or equivalent), NOT `sh -c "<binary> --version"`. Shell expansion opens command-injection if the binary name ever leaks user input (defense in depth even though constraint #1 should prevent it). |
| 5 | **No user-supplied commands during scan** | The scan executes only the registry-known binary list; even if the wizard's UI later accepts a custom `launch_command`, that string is written to `~/.nexus42/agent-host/config.toml` and read by the daemon at next boot — it is NOT executed by the scan. |

## Why This Matters

- **Arbitrary command execution is the worst-case regression.** A scan endpoint that accepts a binary name from the request body and shells out would let any local process that can reach `127.0.0.1:<port>` (or any LAN peer if remote-bind is on) run arbitrary commands as the user.
- **Defense in depth.** Constraint #1 alone is theoretically sufficient (registry is trusted), but constraints #2–#5 catch the case where a future change accidentally widens the input source. Each constraint is independently testable.
- **Observable + bounded.** Bounded concurrency + hard timeout mean the endpoint's worst-case latency is predictable; no UI hang; no resource exhaustion.

## When to Apply

- Any "auto-detect local X" feature where X is an executable (agent client, linter, runtime, build tool).
- Any daemon endpoint that executes a subprocess on behalf of a client request.
- Any code path that takes a binary name from a cache or registry and probes it.

## Examples

- **ACP Agent Detection (V1.94)** — the canonical application. `crates/nexus-acp-host/src/registry.rs::scan_local_installations` enforces all five constraints; `POST /v1/daemon/agent-host/scan` exposes the combined registry + PATH result.
- **Hypothetical future "linter picker"** — same pattern: registry of known linters, PATH probe with the same five constraints, never execute user-supplied strings during the scan phase.

## Anti-patterns

- ❌ `Command::new(format!("{} --version", user_supplied_name))` — shells out, command-injection-prone.
- ❌ Unbounded `join_all(probes)` — can spawn dozens of subprocesses; exhausts FDs/CPU.
- ❌ No per-probe timeout — one hung binary stalls the entire scan.
- ❌ Accepting `launch_command` from the request body of `/scan` — that's the post-scan affordance, not the scan itself; conflating them widens the attack surface.
- ❌ Trusting the registry cache blindly without constraint #1 — if the cache is corrupted or replaced, the binary list could be hostile.

## Testing

The unit tests should cover:
- Registry-known binary not on PATH → `installed: false`.
- Registry-known binary on PATH (mocked) → `installed: true` + parsed `version`.
- Probe timeout (mock a hanging binary) → returns within 2s with `installed: false` (or `version: None`).
- Concurrency cap (mock N+1 binaries where N is the cap) → at most N run simultaneously.

qc2 (security lens) is the canonical reviewer for any change to this code path.
