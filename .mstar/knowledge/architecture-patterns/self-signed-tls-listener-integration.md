---
module: daemon-runtime-transport
date: 2026-07-05
problem_type: integration-pattern
category: architecture-patterns
severity: medium
tags: [tls, rustls, rcgen, axum-server, self-signed-cert, san, tofu, remote-bind, listener]
applies_when: adding TLS to an axum-based local-first daemon that must serve trusted-LAN clients via a self-signed cert + TOFU pinning
---

# Self-Signed TLS Listener Integration (rcgen + rustls + axum-server)

## Context

V1.92 added TLS to the `nexus42` daemon so a remote-bind (non-loopback) listener does not expose the API key and manuscript data in cleartext. The cert is **auto-generated, self-signed, Ed25519**, stored under `~/.nexus42/tls/` (dir `0o700`, key `0o600`), and trusted by GUI clients via **TOFU** (certificate-fingerprint pinning). The integration sits on axum 0.7 + hyper 1 + tokio.

## Guidance

### Library choice: `axum-server` over manual `tokio-rustls`

Use `axum_server::bind_rustls(addr, RustlsConfig::from_pem(cert_pem, key_pem))`. The existing plain-HTTP path is `axum::serve(listener, app)`; `axum-server` returns a TLS **listener** that accepts the **same `app`/router and the same `.with_graceful_shutdown(...)` / `Handle`** wiring. A manual `tokio-rustls` wrap of accepted `TcpStream`s would duplicate the accept/upgrade/spawn loop (~30 lines) for no benefit. **Verify the call site is unchanged except for which listener you pass in.**

### Crypto provider: install exactly once, at startup

`rustls 0.23` requires a crypto provider. Call `rustls::crypto::aws_lc_rs::default_provider().install_default()` **once** at daemon startup (boot), before any `RustlsConfig` is constructed. Missing this → runtime panic on first TLS op. Installing twice → process abort. A `static std::sync::OnceLock` or an early idempotent call in `run_daemon` is the correct shape.

### Cert generation: rcgen Ed25519, idempotent, perms enforced

- `rcgen::KeyPair::generate_for(&PKCS_ED25519)` — smaller keys (32B), faster signing, no curve negotiation.
- Generate **once** on first remote bind; persist PEM cert + key via `nexus-home-layout::tls_*` paths; **reuse** across restarts (idempotent — if both files exist and parse via `rustls-pemfile`, reuse; else regenerate). Corrupt/partial files → regenerate, do not crash.
- **Directory `0o700`, key file `0o600`** — enforce + assert in tests; do not rely on umask.

### CRITICAL — SAN must include the non-loopback bind host

The self-signed cert's `subject_alt_names` must include the **actual non-loopback bind host**, not just loopback. This is the V1.92 W-001 lesson:

- A remote client connecting to `https://192.168.1.42:8420` will run TLS hostname validation against the presented cert.
- If the cert's SAN list contains only `127.0.0.1`/`::1`/`localhost` (the loopback defaults), hostname validation **fails before** the client can fetch the fingerprint → the TOFU first-use flow never starts. **The headline product outcome is silently broken.**
- A test that connects via `127.0.0.1` to a daemon bound on `0.0.0.0` **sidesteps** this (the loopback SAN matches) and will pass while the real remote path is broken. Tests must connect via the **actual non-loopback bind address** or assert the SAN list directly.

**Fix shape**: thread the resolved `bind_host` into cert generation. `build_subject_alt_names(bind_host)`: always emit loopback SANs; add the bind host as `SanType::IpAddress` if it parses as an IP, else `SanType::DnsName`; **skip wildcards** (`0.0.0.0`/`::`) since they are not valid TLS server names.

### Fingerprint = public trust anchor, unauthenticated endpoint

`GET /v1/daemon/runtime/cert-fingerprint` returns the `SHA256:<colon-hex>` of the cert DER. **No auth** — the fingerprint is what clients pin (SSH-host-key model), not a secret. Response shape: `{ fingerprint, algorithm: "sha256", created_at? }`. **Loopback-only daemon (no cert) returns `fingerprint: ""` + `algorithm: "sha256"` + no `created_at`** — do NOT use `Default::default()` (it would emit `algorithm: ""` and violate the schema enum). Distinguish three client-side states: fingerprint-present / loopback-only-empty / fetch-failed.

### Listener parity

The TLS path must receive the **same fully-layered router** as plain HTTP — runtime routes (incl. the unguarded fingerprint + health) + protected routes (behind `require_api_key`) + outer CORS + Origin gate. No middleware drift between listener paths.

## Why This Matters

The SAN bug (W-001) was caught only because QC reviewed the cert-generation code with the TOFU flow in mind; the integration test masked it. The lesson generalises: **for self-signed + hostname-validated TLS, the cert's SAN coverage is a correctness requirement, not cosmetic**, and tests that connect via loopback to a wildcard-bound daemon are insufficient proof. Codifying this prevents the same class of regression on any future listener/cert change.

## When to Apply

- Any addition/change to the daemon TLS listener, cert generation, or SAN logic.
- When writing a test for a remote-bind TLS path: connect via the real bind host, or assert SAN membership directly — never via loopback to a wildcard bind.
- Related: `daemon-api-remote-bind-gate.md` (V1.90) covers the env-var gate; this doc covers the TLS layer that became the **third** gate condition in V1.92 (remote-bind requires key + flag + usable cert, fail-closed).

## Known limitation (deferred)

The cert is reused from `~/.nexus42/tls/` without re-validating SAN vs the **current** bind host. If a user first binds host A, then restarts bound to host B, the persisted cert still carries only A's SANs → client connecting to B fails until the user deletes the TLS dir. Future: detect SAN/bind-host mismatch at load and regenerate. Tracked as `R-V192P0-001`.

## Source

V1.92 P0 (TLS for Remote Bind) + P-1 spike (`tests/tls_spike.rs`) + QC1 W-001 (SAN hostname validation, fixed in `59b947d1`) + QC3 transport lens. Dep stack: `axum-server 0.7` + `rustls 0.23` (aws-lc-rs) + `rcgen 0.13` + `rustls-pemfile 2`.
