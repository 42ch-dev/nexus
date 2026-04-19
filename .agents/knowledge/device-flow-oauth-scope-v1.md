# Device flow OAuth scope (TD-10)

**Source plan:** `v1-tech-debt-cleanup` (Batch D, Task 16 / TD-10)  
**Status:** Active — scope and dependency record

## Current behavior (OSS)

`crates/nexus42d/src/auth/device_flow.rs` implements **local session shapes** and a **stub** `verify_device_code` that returns `Ok(false)` — suitable for offline/dev skeletons, **not** production OAuth against a live IdP.

Production device authorization requires **platform-hosted** OAuth2 device endpoints (authorization, token, refresh), stable base URLs, client registration, and CI-safe test doubles. Those contracts and environments live outside this repository’s default CI graph.

## Milestone decision

**Real platform OAuth** is **not** implemented in the V1.1-era tech-debt C+D cap. Work is **explicitly deferred** until platform auth APIs and runbooks are available for integration and stubbed integration tests.

## Code pointer

Module-level documentation in `device_flow.rs` states the production dependency; keep that file as the first stop for future implementers.

## Related

- [architecture-alignment-review-v1.md](architecture-alignment-review-v1.md) (TD-10 source)
