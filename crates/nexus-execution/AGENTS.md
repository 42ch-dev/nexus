# nexus-execution

The port-driven durable run engine, extracted from `nexus-core` in v1.245. `nexus-core` composes it behind its `execution` feature; the crate contract is [src/lib.rs](src/lib.rs).

- Owns the bounded run driver and its single-flight coordinator (`workflow`), scheduler ownership (`schedules`), the bounded per-run event rings (`run_events`), and the workspace session/snapshot/commit primitives (`session`, `session_commit`, `scope`, `bounds`, `commit_fs`, `authority`, `state_provider`).
- Dependency direction is `nexus-core → nexus-execution`, never back. `tooling/check-graph-pins.sh` pins the edge both ways: absent with `--no-default-features`, exactly one version under `nexus-core --features execution`.
- Outside this crate, always: `CoreService` and its owner state, the `nexus.*` tool dispatch and capability/compute/ops surface, the hosted composition, apply-store authority (`apply`, `CreatorAppliedDocuments`' store reads), the HTTP/SSE transports, OS signals and the SPA.
- Ports, not owner state. The coordinator takes an `AppliedAdmissionPort` (mandatory, never `Option`) and a `RunEventPort`; it holds no DB writes of its own. `admit` receives the caller's `nexus_home` so the intake key and the applied-pin re-comparison read the same home that resolved the preset.
- Feature boundaries: `prompt_executor` is `provider-host`; `test_hooks` is `test`/`test-hooks`. Production builds contain neither, so a test hook is never a production seam.
