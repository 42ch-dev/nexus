---
module: crates/nexus-acp-host (implementation retired in v1.212), apps/nexus-service + crates/nexus-agent-host (surviving scan route and discovery)
date: 2026-07-06
last_updated: 2026-10-11
problem_type: architecture_pattern
category: architecture-patterns
severity: high
tags:
  - acp
  - agent-host
  - path-scan
  - security
  - execution-boundary
  - bounded-concurrency
  - timeout
  - retired-implementation
applies_when:
  - designing any feature that scans the local environment for installed tools/agents or probes PATH for binaries (auto-detect local X where X is an executable)
  - reviewing the retired ACP agent-scan implementation
---

# Local-Environment Scan Safety Boundary (ACP Agent Detection)

**Track**: Knowledge (durable guidance distilled from V1.94 Desktop App Onboarding & IA Pass).

> **Status — implementation retired (v1.212, issue #391).** The concrete
> implementation this document was written about no longer exists: the
> `scan_local_installations` helper family and `LocalInstallation` were deleted
> from `crates/nexus-acp-host/src/registry.rs` once nothing called them. The five
> constraints below are retained as **design guidance for a future scan
> feature**, not as a description of code at HEAD. The
> `POST /v1/daemon/agent-host/scan` route still exists, but it is served by host
> provider discovery (`crates/nexus-core` host query → `crates/nexus-agent-host/src/discovery/path_scan.rs`),
> not by the registry-plus-probe implementation described here; read
> `specs/surfaces/desktop-shell.md` §14 for the contract that route carries today.

## Context

V1.94 shipped `POST /v1/daemon/agent-host/scan` — the daemon-side endpoint powering the Setup Wizard's Step 3 "ACP Agent Detection". That design combined:

1. The existing `RegistryClient` cache (`~/.nexus42/registry/cache.json`, fetched from `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`).
2. A `scan_local_installations` helper in `crates/nexus-acp-host/src/registry.rs` (deleted in v1.212, issue #391) that probed the user's PATH for each registry-known binary name and reported `installed: bool` + best-effort `version`.

The scan's output drives the wizard's default recommendation (first PATH-available agent) and lets the user pick. The user may also supply a custom `launch_command` after the scan, but the scan itself must not execute user input.

The challenge the design had to answer — and the one any scan feature faces: **a
service executing subprocesses (`<binary> --version`) on behalf of the UI**,
mediated by an HTTP endpoint. The security boundary must be airtight — a
malicious actor who can shape the binary name list (e.g. via a compromised
registry cache) must not gain arbitrary command execution.

## Guidance (the pattern)

The pattern is **five normative constraints** (distilled from the V1.94
implementation; `specs/surfaces/desktop-shell.md` §14 restates the scan contract
that remains in force). All five are required; any one missing opens an attack
surface.

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

- **ACP Agent Detection (V1.94; implementation retired in v1.212, issue #391)** — the design this document distils, and the reference application of the five constraints. `scan_local_installations` and the registry-plus-probe wiring are gone; a future scan feature should apply the same constraints. The surviving `POST /v1/daemon/agent-host/scan` route is backed by host provider discovery (`crates/nexus-agent-host/src/discovery/path_scan.rs`) rather than by this design.
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

qc2 (security lens) is the canonical reviewer for any change to a scan code path.
