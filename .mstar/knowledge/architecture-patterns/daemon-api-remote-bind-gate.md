---
module: apps/nexus-service
problem_type: architecture_pattern
category: architecture-patterns
severity: high
date: 2026-07-05
last_updated: 2026-10-12
tags:
  - daemon-api
  - remote-bind
  - security
  - loopback
  - api-key
  - tls
  - boot
---

# Daemon API Remote-Bind Gate

Pattern for allowing the Nexus daemon to bind to a non-loopback interface only when the user has explicitly opted in.

## Context

The Nexus service serves the **Daemon API** over HTTP. By default it binds to loopback (`127.0.0.1`, `::1`, `localhost`) so that only local clients can reach it. A non-loopback bind is opt-in and fail-closed: it requires **all** of

1. `--allow-remote` (explicit intent),
2. `--tls-cert` + `--tls-key` (TLS is mandatory off-loopback — a cleartext non-loopback listener is refused),
3. an API key in `NEXUS42_DAEMON_API_KEY` (`apps/nexus-service/src/config.ts:373-374`).

`--allow-remote` / `--tls-cert` / `--tls-key` are parsed in `apps/nexus-service/src/config.ts:439-446` (cert and key must be supplied together, `config.ts:476-477`). The gate is enforced both at startup and per request; neither depends on `NEXUS_DAEMON_REMOTE_BIND`.

## Decision

Enforce the gate **before the listener is opened** and again on every *guarded* request, failing closed with a clear error. Do not rely on the auth middleware alone: middleware runs per-request and can be bypassed by network-level mistakes or misconfigured proxies. A startup gate prevents accidental exposure entirely.

## Implementation

**Startup gate** — `validateStartupBind` (`apps/nexus-service/src/bind.ts:31-48`) runs before native open/listen. A unix transport is exempt (no TCP bind; its socket file is chmod'd and its parent dir enforced). For HTTP:

- loopback bind host (`isLoopbackBindHost`, `bind.ts:6-25`) → allowed;
- non-loopback without `--allow-remote` → `403 forbidden`;
- non-loopback without `--tls-cert`/`--tls-key` → `403 forbidden`;
- non-loopback without an API key → `401 auth_required`.

**Request gate** — `checkApiKey` (`apps/nexus-service/src/security.ts:83-89`) re-checks on each **guarded** request via `validateBindPolicy` (`security.ts:44-58`): a non-loopback peer without `--allow-remote` is `403 forbidden`, and non-loopback without TLS cert/key is `403 forbidden`. Keyed mode (`authMode == 'keyed_all'`) requires the `x-api-key` header to constant-time-match `config.apiKey`; in keyless mode a non-loopback peer is refused `403 forbidden`, so loopback bypass can never leak to a remote address. The service invokes it only for routes whose tier is not `unguarded` (`apps/nexus-service/src/server.ts:377-389`); explicitly unguarded runtime surfaces (e.g. the cert-fingerprint route, `apps/nexus-service/src/routes.ts:294-301`) stay unauthenticated **by design**.

**Serving** — when both PEMs are present the listener is `node:https` (`createHttpsServer`, `apps/nexus-service/src/server.ts:461-467`); otherwise `node:http`.

## Historical (retired Rust daemon boot gate, deleted v1.193 P2)

The original V1.90 gate lived in the Rust daemon: a pure function `ensure_remote_bind_allowed(host)` checked `NEXUS42_DAEMON_API_KEY` **and** `NEXUS_DAEMON_REMOTE_BIND=1` immediately before `TcpListener::bind` and refused a non-loopback bind otherwise. That HTTP-service path is gone. The **same** env-var pairing survives only in the core **Connect peer-tools lane**, `crates/nexus-core/src/connect/accept.rs:204-228` (`ensure_remote_bind_allowed`): a non-loopback peer-tools bind is permitted only when both `NEXUS42_DAEMON_API_KEY` and `NEXUS_DAEMON_REMOTE_BIND=1` are set. It does **not** gate the Daemon API HTTP surface.

## Why this works

- **Fail-closed**: a missing flag, cert/key, or key stops the service before a socket is opened, and the request gate refuses too.
- **Loopback unaffected**: local development and Tauri/desktop default paths require no extra flags.
- **TLS mandatory off-loopback**: an opt-in remote listener can never run in cleartext, so the API key and manuscript bytes are not exposed on the LAN.
- **Auth middleware remains**: the startup gate protects against accidental bind; the request gate still enforces key presence on every non-loopback **guarded** request.

## Testing pattern

1. **Pure-function unit test** for `isLoopbackBindHost`: `localhost`, `127.x`, `::1` → true; a public/LAN IP → false.
2. **Startup-gate unit test** for `validateStartupBind`:
   - loopback host → allowed with no flags;
   - non-loopback, no `--allow-remote` → refused;
   - non-loopback, `--allow-remote` but no cert/key → refused;
   - non-loopback, `--allow-remote` + cert/key but no `NEXUS42_DAEMON_API_KEY` → refused;
   - non-loopback with all three → allowed.
3. **Env-var serialization**: read `NEXUS42_DAEMON_API_KEY` once at config resolution (`config.ts:373-374`); tests set it before building the config rather than mutating process env mid-test.

## When to apply

Use this pattern whenever a local-first service adds an opt-in remote listener. Do not use it for:
- Services that are intended to be public by default (different model).
- Features that need fine-grained network ACLs (use a reverse proxy or firewall instead).

## References

- Startup gate: `apps/nexus-service/src/bind.ts`
- Request gate: `apps/nexus-service/src/security.ts`
- Config flags/key: `apps/nexus-service/src/config.ts`
- Core Connect peer-tools env gate (live): `crates/nexus-core/src/connect/accept.rs`
- Spec (historical, V1.90; superseded): `.mstar/specs/archived/runtime/2026-09-21-daemon-runtime.md`
- Surface conventions (current): `.mstar/specs/runtime/daemon-api-surface-conventions.md`
