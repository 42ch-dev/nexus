---
module: apps/nexus-service
problem_type: architecture_pattern
category: architecture-patterns
severity: high
date: 2026-07-05
last_updated: 2026-10-25
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
2. an API key in `NEXUS42_DAEMON_API_KEY` (`apps/nexus-service/src/config.ts:373-374`).

TLS (`--tls-cert` + `--tls-key`, supplied together per `config.ts:476-477`) is **optional**
since the 2026-10-22 browser-access change (`93b2043a4`): a plain-HTTP LAN listener with
`--allow-remote` + key is admitted, because browser clients cannot present a client-side
trust anchor and the key plus the tokenized bootstrap URL carry the security weight
(see Request gate below).

`--allow-remote` / `--tls-cert` / `--tls-key` are parsed in `apps/nexus-service/src/config.ts:439-446` (cert and key must be supplied together, `config.ts:476-477`). The gate is enforced both at startup and per request; neither depends on `NEXUS_DAEMON_REMOTE_BIND`.

## Decision

Enforce the gate **before the listener is opened** and again on every *guarded* request, failing closed with a clear error. Do not rely on the auth middleware alone: middleware runs per-request and can be bypassed by network-level mistakes or misconfigured proxies. A startup gate prevents accidental exposure entirely.

## Implementation

**Startup gate** — `validateStartupBind` (`apps/nexus-service/src/bind.ts:31-48`) runs before native open/listen. A unix transport is exempt (no TCP bind; its socket file is chmod'd and its parent dir enforced). For HTTP:

- loopback bind host (`isLoopbackBindHost`, `bind.ts:6-25`) → allowed;
- non-loopback without `--allow-remote` → `403 forbidden`;
- non-loopback without an API key → `401 auth_required`;
- TLS is optional (cleartext LAN binds are admitted with the key).

**Request gate** — `checkApiKey` (`apps/nexus-service/src/security.ts:83-89`) re-checks on each **guarded** request via `validateBindPolicy` (`security.ts:44-58`): a non-loopback peer without `--allow-remote` is `403 forbidden`, and a non-loopback peer over cleartext is refused **only while unauthenticated** (`security.ts:44-65` — `authenticated` becomes true after API-key/session-cookie admission, and is always true for the one-time `/__bootstrap` token route, `server.ts:384-389`). SPA static serving sits behind the origin gate and is key-free (`server.ts:517-520`), which is what lets a fresh LAN browser land on the app shell instead of a TLS interstitial. Keyed mode (`authMode == 'keyed_all'`) requires the `x-api-key` header to constant-time-match `config.apiKey`; in keyless mode a non-loopback peer is refused `403 forbidden`, so loopback bypass can never leak to a remote address. The service invokes it only for routes whose tier is not `unguarded` (`apps/nexus-service/src/server.ts:377-389`); explicitly unguarded runtime surfaces (e.g. the cert-fingerprint route, `apps/nexus-service/src/routes.ts:294-301`) stay unauthenticated **by design**.

**Serving** — when both PEMs are present the listener is `node:https` (`createHttpsServer`, `apps/nexus-service/src/server.ts:461-467`); otherwise `node:http`.

## Connect peer-tools env gate

The core **Connect peer-tools lane** (`crates/nexus-core/src/connect/accept.rs:204-228`, `ensure_remote_bind_allowed`) permits a non-loopback peer-tools bind only when both `NEXUS42_DAEMON_API_KEY` and `NEXUS_DAEMON_REMOTE_BIND=1` are set. It does **not** gate the Daemon API HTTP surface.

## Why this works

- **Fail-closed**: a missing flag, cert/key, or key stops the service before a socket is opened, and the request gate refuses too.
- **Loopback unaffected**: local development and desktop default paths require no extra flags.
- **Key-backed cleartext is bounded**: a plain-HTTP LAN listener requires explicit `--allow-remote` plus the key, and unauthenticated cleartext requests are refused; TLS remains available (and recommended) whenever certificates exist, but is no longer a startup precondition.
- **Auth middleware remains**: the startup gate protects against accidental bind; the request gate still enforces key presence on every non-loopback **guarded** request.

## Testing pattern

1. **Pure-function unit test** for `isLoopbackBindHost`: `localhost`, `127.x`, `::1` → true; a public/LAN IP → false.
2. **Gate-matrix unit test** (`apps/nexus-service/tests/i-000061-gate-matrix.test.mjs`, pure functions, no socket):
   - `validateStartupBind`: unix exempt; loopback allowed keyless/TLS-less; non-loopback without `--allow-remote` → 403; non-loopback without key → 401; **plain-HTTP LAN row (`--allow-remote` + key, no TLS) admitted**;
   - `validateBindPolicy`: non-loopback without `--allow-remote` → 403; TLS admits unauthenticated; **unauthenticated no-TLS → 403**; **authenticated no-TLS → admitted**; `/__bootstrap` needs no TLS.
   Red-on-revert: reintroducing a TLS startup precondition fails the plain-HTTP LAN row; dropping the `!authenticated` guard fails the unauthenticated-no-TLS row.
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
- Surface conventions (current): `.mstar/specs/active/runtime/daemon-api-surface-conventions.md`
