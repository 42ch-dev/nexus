---
module: daemon-runtime
problem_type: architecture_pattern
category: architecture-patterns
severity: high
date: 2026-07-05
plan_id: 2026-07-05-v1.90-closure
tags: [daemon-api, remote-bind, security, loopback, api-key, axum, boot]
---

# Daemon API Remote-Bind Gate

Pattern for allowing the Nexus daemon to bind to a non-loopback interface only when the user has explicitly opted in.

## Context

The Nexus daemon serves the **Daemon API** over HTTP. By default it binds to loopback (`127.0.0.1`, `::1`, `localhost`) so that only local clients can reach it. V1.90 made the daemon "remote-ready": an author may choose to bind to a non-loopback address, but only when both of the following are true:

1. An API key is configured (`NEXUS42_DAEMON_API_KEY`).
2. Remote bind is explicitly enabled (`NEXUS_DAEMON_REMOTE_BIND=1`).

Without both conditions, the daemon must refuse to start before opening the listener.

## Decision

Enforce the gate at **transport resolution time**, immediately before `TcpListener::bind`, and fail closed with a clear error. Do not rely on the auth middleware alone: middleware runs per-request and can be bypassed by network-level mistakes or misconfigured proxies. A boot-time gate prevents accidental exposure entirely.

## Implementation

```rust
fn is_loopback_host(host: &str) -> bool {
    host == "localhost"
        || host.trim().parse::<IpAddr>().map(|ip| ip.is_loopback()).unwrap_or(false)
}

fn ensure_remote_bind_allowed(host: &str) -> anyhow::Result<()> {
    if is_loopback_host(host) {
        return Ok(());
    }
    let key_set = std::env::var("NEXUS42_DAEMON_API_KEY").map(|s| !s.is_empty()).unwrap_or(false);
    let remote_enabled = std::env::var("NEXUS_DAEMON_REMOTE_BIND").as_deref() == Ok("1");
    if key_set && remote_enabled {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "remote bind to {host} requires both NEXUS42_DAEMON_API_KEY and NEXUS_DAEMON_REMOTE_BIND=1"
        ))
    }
}
```

Call site in `run_daemon`:

```rust
if let Transport::Http { ref host, port } = transport {
    ensure_remote_bind_allowed(host)?;
    let listener = TcpListener::bind((host.as_str(), port)).await?;
    ...
}
```

## Why this works

- **Fail-closed**: a missing key or flag stops the daemon before a socket is opened.
- **Loopback unaffected**: local development and Tauri/desktop default paths require no extra env vars.
- **Dual signal**: requiring both a secret (API key) and an explicit intent flag (`=1`) prevents a single misconfiguration from opening the surface.
- **Auth middleware remains**: the gate protects against accidental bind; the existing `AuthMode::KeyedAll` vs `KeylessLocalhost` middleware still enforces key presence on non-loopback requests.

## Testing pattern

1. **Pure-function unit test** covering the truth table:
   - loopback host → allowed regardless of env
   - non-loopback host, no key → rejected
   - non-loopback host, key only → rejected
   - non-loopback host, key + flag → allowed
2. **Boot-path integration test** that calls `run_daemon()` with `host: "0.0.0.0"` and asserts the daemon fails before listening when env vars are missing, then succeeds when both are set.
3. **Env-var serialization**: guard `std::env::set_var` / `remove_var` in tests with a `LazyLock<Mutex<()>>` to prevent parallel tests from observing torn values.

## When to apply

Use this pattern whenever a local-first service adds an opt-in remote listener. Do not use it for:
- Services that are intended to be public by default (different model).
- Features that need fine-grained network ACLs (use a reverse proxy or firewall instead).

## V1.90 references

- Implementation: `crates/nexus-daemon-runtime/src/boot.rs`
- Auth middleware: `crates/nexus-daemon-runtime/src/api/auth_middleware.rs`
- Spec: `.mstar/knowledge/specs/daemon-runtime.md`
- Surface conventions: `.mstar/knowledge/specs/daemon-api-surface-conventions.md`
