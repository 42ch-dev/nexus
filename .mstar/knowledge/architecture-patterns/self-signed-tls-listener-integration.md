---
module: apps/nexus-service
date: 2026-07-05
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
tags:
  - tls
  - https
  - self-signed-cert
  - san
  - tofu
  - remote-bind
applies_when:
  - adding TLS to the local-first daemon so a non-loopback listener does not expose the API key and manuscript data in cleartext, and writing a self-signed cert that trusted-LAN clients pin via TOFU
---

# Self-Signed TLS Listener Integration (node:https)

## Context

TLS exists on the daemon so a remote-bind (non-loopback) listener does not expose the API key and manuscript data in cleartext. In the current standalone TypeScript service the listener is **`node:https`**: `apps/nexus-service/src/server.ts:8-10` imports `createServer as createHttpsServer` from `node:https`, and `server.ts:461-467` selects it whenever both `--tls-cert` and `--tls-key` are supplied:

```ts
const server =
  config.tlsCert && config.tlsKey
    ? createHttpsServer(
        { cert: config.tlsCert, key: config.tlsKey, ServerResponse: BoundedSseHttpResponse },
        handler,
      )
    : createHttpServer({ ServerResponse: BoundedSseHttpResponse }, handler);
```

`config.ts` requires `--tls-cert` and `--tls-key` together (`config.ts:476-477`) and reads the PEM files into `config.tlsCert` / `config.tlsKey`. The certificate itself is supplied by the operator/desktop (self-signed + trusted by GUI clients via **TOFU** certificate-fingerprint pinning).

Historical (retired v1.193 P2): the Rust daemon auto-generated a self-signed Ed25519 cert on first remote bind, stored under `~/.nexus42/tls/` (dir `0o700`, key `0o600`), and served it through `axum-server`/`rustls`; that crate is gone and the current service consumes operator-provided PEM.

## Guidance

### Listener choice: `node:https` over a separate TLS server

Build the TLS listener with `createHttpsServer({ cert, key }, handler)`. The existing plain-HTTP path is `createHttpServer({ ... }, handler)`; both take the **same `handler`** (and the same `BoundedSseHttpResponse` class), so the TLS variant requires no separate accept/upgrade loop. **Verify the call site is unchanged except for which server factory you pass in.**

### Cert generation is out of the service

The current service does not generate keys or certificates; it reads `--tls-cert`/`--tls-key` PEM. The Ed25519 `rcgen` generation, the rustls crypto-provider install, and the `~/.nexus42/tls/` persistence described in earlier revisions were the retired Rust daemon (historical, deleted v1.193 P2) - do not reintroduce them here. Whatever produces the PEM is still responsible for the SAN rule below.

### CRITICAL — SAN must include the non-loopback bind host

The self-signed cert's `subject_alt_names` must include the **actual non-loopback bind host**, not just loopback. This is the V1.92 W-001 lesson, and it applies to whichever side generates the cert:

- A remote client connecting to `https://192.168.1.42:8420` will run TLS hostname validation against the presented cert.
- If the cert's SAN list contains only `127.0.0.1`/`::1`/`localhost` (the loopback defaults), hostname validation **fails before** the client can fetch the fingerprint -> the TOFU first-use flow never starts. **The headline product outcome is silently broken.**
- A test that connects via `127.0.0.1` to a daemon bound on `0.0.0.0` **sidesteps** this (the loopback SAN matches) and will pass while the real remote path is broken. Tests must connect via the **actual non-loopback bind address** or assert the SAN list directly.

**Fix shape** (unchanged): thread the resolved `bind_host` into cert generation and emit loopback SANs plus the bind host; **skip wildcards** (`0.0.0.0`/`::`) since they are not valid TLS server names.

### Fingerprint = public trust anchor, unauthenticated endpoint

`GET /v1/daemon/runtime/cert-fingerprint` (unguarded: `apps/nexus-service/src/routes.ts:296-300`) returns the `SHA256:<colon-hex>` of the cert DER. `fingerprintFromPem` (`apps/nexus-service/src/tls.ts:4-13`) computes it with `node:crypto` (`new X509Certificate(pem)` + `createHash('sha256').update(cert.raw)`); `created_at` comes from the cert file mtime via `certCreatedAtFromMtime` (`tls.ts:14-16`). **No auth** — the fingerprint is what clients pin (SSH-host-key model), not a secret. Response shape: `{ fingerprint, algorithm: "sha256", created_at? }`. **Loopback-only daemon (no cert) withholds the fingerprint** (`lifecycle.ts:118-128`: the record is only built when `config.tlsCert` is set). Distinguish three client-side states: fingerprint-present / loopback-only-empty / fetch-failed.

### Listener parity

The TLS path must receive the **same fully-layered handler** as plain HTTP - runtime routes (incl. the unguarded fingerprint + health) + protected routes (behind `checkApiKey`) + outer CORS + Origin gate. No middleware drift between listener paths.

## Why This Matters

The SAN bug (W-001) was caught only because QC reviewed the cert-generation code with the TOFU flow in mind; the integration test masked it. The lesson generalises: **for self-signed + hostname-validated TLS, the cert's SAN coverage is a correctness requirement, not cosmetic**, and tests that connect via loopback to a wildcard-bound daemon are insufficient proof. Codifying this prevents the same class of regression on any future listener/cert change.

## When to Apply

- Any addition/change to the daemon TLS listener, cert handling, or SAN logic.
- When writing a test for a remote-bind TLS path: connect via the real bind host, or assert SAN membership directly — never via loopback to a wildcard bind.
- Related: `daemon-api-remote-bind-gate.md` (V1.90) covers the env-var gate; this doc covers the TLS layer that became the **third** gate condition in V1.92 (remote-bind requires key + flag + usable cert, fail-closed).

## Known limitation (deferred)

The persisted cert is not re-validated against the **current** bind host at load. If a user first binds host A, then restarts bound to host B, the cert still carries only A's SANs -> a client connecting to B fails until the cert is regenerated. Future: detect SAN/bind-host mismatch at load and regenerate. Tracked as `R-V192P0-001`.

## Source

V1.92 P0 (TLS for Remote Bind) + P-1 spike (`tests/tls_spike.rs`) + QC1 W-001 (SAN hostname validation, fixed in `59b947d1`) + QC3 transport lens. Historical (retired v1.193 P2) Rust dep stack: `axum-server 0.7` + `rustls 0.23` (aws-lc-rs) + `rcgen 0.13` + `rustls-pemfile 2`; current listener is `node:https` (`server.ts:8-10,461-467`).
