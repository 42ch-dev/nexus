---
module: apps/nexus-service
date: 2026-07-05
last_updated: 2026-10-14
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
tags:
  - csrf
  - security
  - auth
  - origin-allowlist
  - header-key
  - remote-bind
  - local-first
applies_when:
  - considering adding a CSRF token framework (double-submit cookie / synchronizer token / SameSite session) to a Daemon API that authenticates remote clients via a custom request header
---

# Header-Key Auth Is Its Own CSRF Defence — Don't Add a Token Framework

## Context

V1.90 made the then-integrated `nexus42` daemon opt-in **remote-bind** capable (`NEXUS42_DAEMON_API_KEY` + `NEXUS_DAEMON_REMOTE_BIND=1` for non-loopback binds) — historical: that host was deleted in v1.193 P2, and `NEXUS_DAEMON_REMOTE_BIND=1` now gates the separate core Connect peer-tools bind, not this HTTP surface. `apps/nexus-service` requires `--allow-remote` + `--tls-cert`/`--tls-key` + `NEXUS42_DAEMON_API_KEY` for a non-loopback bind. V1.86 had already closed the localhost trust-boundary attack chain with an Origin allowlist + `require_allowed_origin` middleware, and explicitly deferred a CSRF token framework as a non-goal pending threat-model growth.

The natural reflex once remote-bind shipped was: *"now that browsers can reach the daemon remotely, we need CSRF tokens."* V1.92 evaluated that reflex and rejected it — the existing model already covers CSRF without a token layer.

## Guidance

When the Daemon API authenticates **remote** clients via a **custom request header** (`X-API-Key`) rather than a cookie/session, **a separate CSRF token framework is redundant**. The defence is already complete:

1. A state-changing request must carry `X-API-Key` (a custom header).
2. A cross-origin malicious page **cannot** set a custom header on a `fetch`/XHR without triggering a **CORS preflight** (`OPTIONS`).
3. The V1.86 Origin allowlist is enforced by withholding permission, not by rejecting the preflight: `writeCors` in `apps/nexus-service/src/server.ts` sets `Access-Control-Allow-Origin` only when the request's `Origin` is in `config.allowedOrigins`, and the `OPTIONS` preflight path answers `204` before the auth gates run — so a preflight from a non-allowlisted origin gets **no CORS grant** and the browser withholds the real request. A non-`OPTIONS` request whose `Origin` is present and not allowlisted is rejected `403 forbidden` by `checkOrigin` in `apps/nexus-service/src/security.ts`, the successor to the retired `require_allowed_origin` middleware.
4. Therefore a malicious site can neither read responses nor forge a state-changing request with the required header.

This is the complete CSRF defence for the header-key model. Adding a double-submit cookie or synchronizer token on top would add maintenance surface (cookie issuance, token storage, expiry, SameSite config) for **no incremental security** — those mechanisms exist to defend cookie-based auth, which the daemon does not use.

**The fingerprint endpoint** (`GET /v1/daemon/runtime/cert-fingerprint`) is deliberately **unauthenticated**: the cert fingerprint is a **public trust anchor** (like an SSH host key), not a secret. It is what clients pin via TOFU; it carries no session/key/path data. No auth on it is correct, not an oversight.

## Why This Matters

A future agent or reviewer seeing "no CSRF tokens" on a remotely-reachable API will reach for the familiar remedy. Recording the rationale here (and in the archived `daemon-runtime.md` §16.3) prevents a well-intentioned but redundant CSRF layer from being added — which would imply a session model the daemon deliberately does not have, and would muddy the auth boundary.

## When to Apply

- **Keep the non-goal**: as long as remote auth is header-key based and the Origin allowlist gates preflight, do not add CSRF tokens. Any future proposal to add them should engage with this rationale explicitly and name the concrete new threat — not " defence-in-depth" in the abstract.
- **Re-open this decision only if**: the daemon adopts a cookie/session auth model (then CSRF tokens become relevant), OR the threat model grows to include a vector the header-key + Origin gate does not cover (name it).
- **TOFU note**: the remote client's trust of the self-signed cert is a separate (transport) concern handled by certificate-fingerprint pinning, not by CSRF tokens. See `self-signed-tls-listener-integration.md`.

## Examples

- **Sound (current model)**: remote web-app/desktop client stores the API key in client storage (localStorage / OS keychain), sends it as `X-API-Key` header to the remote daemon. Cross-origin attack blocked at preflight by Origin allowlist. No CSRF token needed.
- **Would re-open the decision**: if a future "remote author session" feature issued a session cookie after login, the cookie path would re-introduce the CSRF surface this pattern does not cover — at that point a CSRF defence scoped to the cookie-auth routes would become relevant.

## Source

V1.92 iteration (Remote-Access Hardening), grill-me direction-lock + `daemon-runtime.md` §16.3 (archived; CSRF defence by header-key). QC2 (security lens) independently confirmed: "Custom header triggers CORS preflight; `require_allowed_origin` runs before `require_api_key`; no state-changing mutation reachable without the header in KeyedAll mode."
