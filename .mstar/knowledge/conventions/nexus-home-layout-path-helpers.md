---
module: nexus-home-layout, nexus-spoke-adapter
date: 2026-08-04
problem_type: knowledge
category: conventions
severity: low
plan_id: 2026-08-04-v1.148-p4-connect-dogfood-sweep-and-specs-alignment
tags: [nexus-home-layout, path-helpers, device-id, raw-home, convention]
applies_when: writing or calling a path helper under `nexus-home-layout`; adding a new `~/.nexus42/...` artifact path
---

# `nexus-home-layout` path helpers take **raw home** — never pre-join `.nexus42`

## Context

V1.148 P4 dogfood caught **F-1 (medium)**: `get_or_create_device_id(home)` joins `.nexus42/device-id` internally, but every call site was passing the **nexus home** (`~/.nexus42`) instead of the raw user home (`$HOME`). Result: the device-id file was written to `~/.nexus42/.nexus42/device-id` (double-nested), the canonical path was never populated, and the host_id (used as `HostCapabilityManifest.host_id` + cloud-sync `X-Device-ID`) would churn on upgrade when the canonical path was later canonicalized.

## Guidance

- **Convention**: every `nexus-home-layout` path helper takes the **raw user home** (`$HOME`, e.g. from `dirs::home_dir()` or `user_home_dir()`) and joins `.nexus42` itself. Callers MUST NOT pre-join `~/.nexus42`.
  ```rust
  // CORRECT
  let id = get_or_create_device_id(dirs::home_dir().unwrap())?;            // joins .nexus42 inside
  // WRONG — causes double nesting
  let nexus_home = config::nexus_home();                                    // already $HOME/.nexus42
  let id = get_or_create_device_id(&nexus_home)?;                           // -> $HOME/.nexus42/.nexus42/...
  ```
- The contract belongs in the helper's doc comment (`Callers MUST pass the raw user home; this fn joins .nexus42 internally.`) so the next caller can't get it wrong at the call site.
- When a helper's parameter is named after the value it expects, name it for the **raw** input (`home`, `user_home`) — not `nexus_home` (that name lies about what to pass and is the exact confusion vector that caused F-1 for the sibling identity/allowlist key paths).

## Why this matters

- The double-nesting bug is **silent**: it writes a valid file in the wrong place; nothing crashes; the canonical path is simply absent. It surfaces only as identity churn (a new host_id appears after the path is fixed) or as a mismatch between two surfaces that resolve the id differently.
- Path helpers in `nexus-home-layout` are the SSOT for `~/.nexus42/...` artifacts; the raw-home contract is what keeps them composable. One mis-named parameter or one pre-joining caller reintroduces a whole class of "wrong directory" bugs.

## When to apply

- Any time you add a new `~/.nexus42/...` artifact path (key files, allowlists, caches, device-id, …) — add it as a `nexus-home-layout` helper that takes raw home.
- Any time you call such a helper — pass `dirs::home_dir()` (or the equivalent raw-home resolver), never a pre-joined `.nexus42` path.
- Code review of path-helper call sites: grep for the helper name and confirm each caller passes raw home.

## Examples

- V1.148 P4 F-1 fix: `crates/nexus-home-layout/src/device_id.rs` (`device_id_path(home)` joins `.nexus42`; doc contract), `apps/nexus42/src/main.rs`, `crates/nexus-spoke-adapter/src/adapter/host_manifest_port.rs`, `apps/nexus42/src/commands/connect/mod.rs` all pass raw `dirs::home_dir()`.
- Sibling helpers that already follow the convention: `connect_dir(home)`, `tls_key_path(home)` (canonical `$HOME/.nexus42/...` output).
- Pre-1.0 note: a path fix that changes the canonical location is accepted without migration (one-time identity churn); record it in `crates/nexus-home-layout/AGENTS.md` path-history so future readers understand.
