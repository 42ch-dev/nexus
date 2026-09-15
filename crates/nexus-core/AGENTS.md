# nexus-core

Owned transport-neutral World KB service for graph/patch/candidates/changes.

- Public API: `CoreService`, `Principal`, `CoreOpenOptions`, `CoreAccess`, `CoreError`.
- No daemon, Axum, napi, orchestration, or SQL pool in the public surface.
- Depends on `nexus-spoke-adapter` with `default-features = false` (no WASM compute).
