# Spec — Native agent provider registration in daemon HostManager (P1)

> **Iteration:** V1.127 · **Plan:** `2026-07-20-v1.127-p1-native-agent-provider-registration`
> **Status:** PM draft — pending product-manager seat 1 + architect seat 2 + writing-specialist seat 3.
> **Source evidence:** Predictive scan by `explore` subagent (V1.127 Phase 1); V1.116 residual `R-V1116P0QA-001` (11-iteration roll-forward).

## Problem

`crates/nexus-daemon-runtime/src/boot.rs:721` constructs the daemon's `HostManager` via `HostManager::new()`, which creates an empty manager. The `register_provider(...)` method is never called. Both `CodexNativeProvider` and `ClaudeCliProvider` exist in the codebase and are path-scanned (`path_scan.rs::KNOWN_COMMANDS` includes both), but they are never wired into the daemon's HostManager. The `AgentHostSubsystem` docstring even says "with providers already registered" (agent_host.rs:44-45) — that invariant has never been met for native providers.

A manual tester with `codex` or `claude` CLI installed locally (very likely — they are the maintainer) opens the AgentPicker from the sidebar submenu and sees an empty or stale agent list. Creating a session against a native agent fails because the provider is not registered. This is the V1.116 residual `R-V1116P0QA-001`, which has rolled forward 11 iterations (V1.116 → V1.126).

## User value

This is the V1.127 down-payment on the **Harness pillar** (one of STRATEGY's Three Pillars): the daemon finally registers the native agent providers (Codex + Claude) that already exist in the codebase but have never been wired into the `HostManager`. The V1.116 residual `R-V1116P0QA-001` — "native agents don't show up in the AgentPicker" — has rolled forward **11 iterations** (V1.116 → V1.126) because every prior iteration deprioritized it for new feature work. V1.127 pays that debt.

**After V1.127**, a dogfood tester with `codex` or `claude` CLI installed locally opens the AgentPicker and sees the discovered native agents (not an empty list). Selecting one and creating a session invokes the provider's session-create flow — the discovery half of the Harness pillar loop is now functional.

**Scope-aware promise (per NG-13):** the V1.127 user-visible fix is the *discovery flow* — agents appear in the picker and the daemon correctly hands off to the provider. If a provider's *internal* session-create handshake has a latent bug (e.g. CodexNativeProvider's specific handshake shape is broken), that becomes a V1.128+ plan candidate — P1 does not open up provider internals in this iteration. The 11-iteration roll-forward ends because the wiring debt is paid, not because every provider is certified bug-free.

## Scope

**In scope (P1):**

- T1 Register `CodexNativeProvider` + `ClaudeCliProvider` via `manager.register_provider(...)` in `crates/nexus-daemon-runtime/src/boot.rs` after `HostManager::new()`, before the agent host subsystem facade is constructed.
- T2 End-to-end verification: path-scan coverage test, agent-list integration test, session-create integration test (or manual QA fallback).

**Out of scope (roadmap):**

- `CodexNativeProvider` / `ClaudeCliProvider` internal logic changes (path-scan, session creation, etc.) — see V1.127 NG-13. If T2 discovers latent bugs in the providers themselves, those become V1.128+ plan candidates.
- `AgentPicker` chrome refactor — see V1.127 NG-12.
- ACP provider registration (separate concern; ACP is already wired if applicable).
- New schemas or wire contracts — see V1.127 NG-16.

## Acceptance criteria

See compass `## Acceptance Criteria` AC-V1127-7. Both T1 and T2 contribute to AC-V1127-7.

## Architecture decisions (PM proposed — pending architect seat 2)

- **`wire_contracts_changed: false`** — providers already exist; only boot wiring changes.
- **Provider types NOT modified** (NG-13) — only registered.
- **Registration order** — AFTER path-scan completes, BEFORE `AgentHostSubsystem::new(manager)` (or equivalent facade constructor).
- **Missing-CLI resilience** — if a provider's `default_config()` (or equivalent constructor) panics when its CLI is absent (it should not, but verify), wrap registration in `match`/`if let Ok(...)` that logs a warning and skips — do NOT fail boot if `codex`/`claude` is not installed.
- **INFO-level boot log** — log which providers were registered (and how many were discovered via path scan) so boot output is debuggable.
- **Session-create verification fallback** — if stubbing the codex/claude session-create handshake is infeasible in an automated test, T2 records the manual-verification step in `## QA Gate Summary` for the QA gate to execute. P1 still ships (the discovery flow is the user-visible fix).

## Open questions (resolved — architect seat 2)

- **AQ-1 (T1 — resolved):** Both providers have `default_config()`:
  - `CodexNativeProvider::default_config()` — `crates/nexus-agent-host/src/providers/native_cli/codex.rs:122` — no-arg factory, returns `Self` with `provider_id = "codex-native"`, `command = "codex"`.
  - `ClaudeCliProvider::default_config()` — `crates/nexus-agent-host/src/providers/native_cli/claude.rs:197` — no-arg factory, returns `Self` with `provider_id = "claude-native"`, `command = "claude"`, `args = ["--print"]`.
  - **IMPORTANT:** The struct name is `ClaudeCliProvider`, NOT `ClaudeNativeProvider` as written in the PM draft. All plan/spec references corrected to `ClaudeCliProvider`.
  - Both implement `ProviderAdapter`: `impl ProviderAdapter for CodexNativeProvider` (codex.rs:530), `impl ProviderAdapter for ClaudeCliProvider` (claude.rs:432).
  - `default_config()` does NOT panic when the CLI is absent — both use the `which` crate for PATH lookup at probe time and return `HostError::provider_unavailable` gracefully (codex.rs:544-579, claude.rs:446-481).
- **AQ-2 (T1 — resolved):** `HostManager::register_provider(&self, adapter: Arc<dyn ProviderAdapter>)` — `crates/nexus-agent-host/src/core/manager.rs:116`. Takes `Arc<dyn ProviderAdapter>` (trait object). Both `CodexNativeProvider` and `ClaudeCliProvider` implement `ProviderAdapter`, so wrapping in `Arc::new(...)` is correct.
- **AQ-3 (T1 — resolved):** **REPLACE** — `register_provider` uses `HashMap::insert` (manager.rs:120), which silently replaces the previous entry for the same `provider_id`. No panic, no skip. Duplicate registration on re-boot is safe.
- **AQ-4 (T2 — resolved):** There is a PATH-scan test pattern but **no existing integration test** for "boot daemon with stubbed CLI on PATH":
  - `crates/nexus-agent-host/src/discovery/path_scan.rs:177-219` — `scan_custom_path()` allows injecting custom PATH dirs for deterministic tests (behind `#[cfg(test)]`).
  - `crates/nexus-daemon-runtime/tests/` has 34 integration test files but none stub a CLI on PATH.
  - `crates/nexus-agent-host/tests/` directory does NOT exist.
  - T2 establishes the pattern: a unit test in `crates/nexus-agent-host/src/providers/native_cli/` verifying `default_config()` + `register_provider()` (does not require booting the daemon — that's the integration test in `crates/nexus-daemon-runtime/tests/`).
- **AQ-5 (T2 — resolved):** The agent-list endpoint is `GET /v1/daemon/agent-host/providers` — **NOT** `/v1/daemon/agents`. `crates/nexus-daemon-runtime/src/api/mod.rs:45-47` routes `GET /v1/daemon/agent-host/providers` → `handlers::agent_host::list_providers`. All plan references corrected from the assumed `/v1/daemon/agents`.

## Dependencies

- T1 → T2 (T2 builds on T1's registered manager).
- No upstream dependencies on P0 (P0 and P1 touch disjoint file trees).

## Risks

See compass `## Risk Register` rows 6–8.
