---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-22-v1.56-df29-registry-refresh"
verdict: "Approve with comments"
generated_at: "2026-06-22"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist (Reviewer #1 — Architecture coherence and maintainability risk)
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek-v4-flash
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-22T23:59:00+08:00

## Scope
- plan_id: 2026-06-22-v1.56-df29-registry-refresh
- Review range / Diff basis: a264c383..d3a03e06
- Working branch (verified): iteration/v1.56
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 13 (630 insertions, 31 deletions)
- Commit range: a264c383..d3a03e06
- Tools run: git diff --stat, git diff per file, manual code review, cargo check -p nexus-orchestration (pass), cargo check -p nexus-daemon-runtime (pass), cargo check -p nexus42 (pass); cargo test (pre-existing sqlx cache issue — not P1-scope)

## Findings

### 🔴 Critical

None. No data corruption, security regression, or critical correctness defects identified. No scope creep into DF-31/DF-42 (P0) or DF-56 (P2/P3) territory.

### 🟡 Warning

#### F-101: Breaking output schema change — `agentCount` renamed to `capabilityCount`

- **Severity**: Medium
- **Source**: `crates/nexus-contracts/src/local/orchestration/mod.rs` — `RegistryRefreshOutput` struct diff
- **Observation**: The P0 stub `RegistryRefreshOutput` had `agent_count: u32` (serialized to JSON as `agentCount`). P1 replaces it with `capability_count: u32` (`capabilityCount`). The semantic change is intentional (counting capabilities, not agents), but this is a **breaking wire change** — any ACP client or external agent that parsed the old `agentCount` field will get `undefined`/`null` and miss the renamed field.
- **Impact**: No in-repo consumer of `agentCount` was found (the stub always returned 0), so internal compatibility is unbroken. However, this is a public capability exposed over ACP; external clients consuming the old output shape will break silently.
- **Recommendation**: Restore `agent_count: u32` as a deprecated/compat field alongside `capability_count` (same value) for one release cycle, or explicitly document in `acp-capability-set.md` §4.7A that the schema is pre-release and may change without notice. Since V1.x is pre-release (per `AGENTS.md` §Pre-release Development), breaking changes are permitted, but a compat field costs nothing and prevents breakage.
- **Scope**: Architecture, backward compatibility

#### F-102: Global mutable state for CDN configuration (`RwLock<Option<CdnConfig>>`)

- **Severity**: Medium
- **Source**: `crates/nexus-orchestration/src/capability/builtins/registry.rs` — `CDN_CONFIG` static
- **Observation**: The CDN URL, timeout, and retry count are stored in a module-level `static CDN_CONFIG: RwLock<Option<CdnConfig>>` — a global mutable singleton. Set once at daemon boot via `set_cdn_config()`, read on every capability invocation. Test isolation requires `#[serial_test::serial]` on every test that touches global state. This pattern:
  1. Prevents constructing independent capability instances with different configs
  2. Introduces implicit coupling between daemon boot order and capability behavior
  3. Requires serialised test execution, which slows CI
  4. Makes it impossible to run two `RegistryRefresh` instances with different URLs in the same process
- **Impact**: Moderate maintainability tax. The `serial_test` dependency must be managed; test ordering failures would be hard to debug.
- **Recommendation**: Refactor to inject `CdnConfig` through the `Capability` trait or a factory pattern (e.g., `RegistryRefresh::with_cdn(config) -> Self`) instead of a static global. Acceptable for V1.56 given scope; recommend registering as residual for post-V1.56 architectural hygiene.
- **Scope**: Architecture, testability, maintainability

#### F-103: Host handler hardcodes `force: false` — schema/behavior mismatch

- **Severity**: Medium
- **Source**: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs` — `execute_registry_refresh` function
- **Observation**: The handler always constructs `serde_json::json!({"force": false})` regardless of the incoming `ToolExecuteRequest` parameters. The capability registration schema documents `force?:bool` as an input, but this parameter is silently ignored in the ACP handler path. The `RegistryRefresh::run()` method also ignores the deserialized input (bound to `_input`). The `force` parameter exists in the wire contract (`RegistryRefreshInput`) but is dead code in both the handler and the capability implementation.
- **Impact**: Callers passing `force: true` receive the same output as `force: false`. The `force` semantic is documented but unimplemented — this is a correctness gap that will confuse ACP clients.
- **Recommendation**: Either:
  (a) Wire `force` through from the `ToolExecuteRequest` to `cap.run(...)` — simple, < 5 lines change, or
  (b) Remove `force` from `RegistryRefreshInput` and `request_schema_ref` if it has no planned use.
  Current state (documented but ignored) is the worst of both worlds.
- **Scope**: API correctness, schema discipline

### 🟢 Suggestion

#### F-104: Schema references use non-standard JSON Schema notation (pre-existing convention)

- **Severity**: Low
- **Source**: `crates/nexus-daemon-runtime/src/capability_registry.rs` — `request_schema_ref: r#"{"force?":"bool"}"#`
- **Observation**: The `request_schema_ref` value `{"force?":"bool"}` is not valid JSON Schema — the `?` suffix for "optional" and `"bool"` for type are colloquial shorthands. This matches a **pre-existing project convention** (all other entries use the same pattern, e.g., `{"title?":"string"}`). However, it means these "schema refs" cannot be mechanically validated or used for codegen.
- **Impact**: Low — these inline schemas are documentation-oriented rather than machine-consumed. But if a future plan wants to use them for codegen or validation, they would fail.
- **Recommendation**: Consider migrating registry schema refs to proper JSON Schema in a follow-up plan, or accept that they are human-readable annotations only and document this convention. Not a P1 blocker.
- **Scope**: API documentation quality, codegen readiness

#### F-105: Embedded snapshot diverges from `acp-capability-set.md` (missing publish + profile IDs)

- **Severity**: Low
- **Source**: `crates/nexus-orchestration/src/capability/builtins/registry.rs` — `REGISTRY_SNAPSHOT_CAPABILITIES`
- **Observation**: The embedded snapshot (31 IDs) is documented as "Updated per release to match the logical catalog in `acp-capability-set.md`." However, the snapshot omits:
  - `nexus.publish.chapter` (§4.6)
  - `nexus.publish.story` (§4.6)
  - `nexus.profile.minimal`, `nexus.profile.writer`, `nexus.profile.publisher` (§4.0 Profiles)
  This creates a **drift risk**: if an agent queries `registry.refresh` to discover available capabilities, it won't see publish or profile IDs.
- **Impact**: Low — profiles may be meta-capabilities and publish may be platform-only. But the omission violates the stated design goal of mirroring the spec catalog.
- **Recommendation**: Either add the missing IDs with a comment explaining their scope, or update the code comment to document the selection criteria (e.g., "runtime-available capabilities only; profiles and publish are meta/platform and excluded").
- **Scope**: Data completeness, spec compliance

#### F-106: No upfront CDN URL validation at CLI parse time

- **Severity**: Low
- **Source**: `crates/nexus42/src/commands/daemon/mod.rs` — `--cdn-url` flag
- **Observation**: The `--cdn-url` flag accepts any string with no validation (no URL parsing, no scheme check, no host validation). An invalid URL (e.g., `--cdn-url "not-a-url"`) will only fail after a 10s timeout + retries when the first `registry.refresh` call is made. This is a poor UX for what could be a quick CLI-time error.
- **Impact**: Low — misconfiguration is caught eventually, but with significant latency.
- **Recommendation**: Add a `url::Url::parse()` check at argument parsing time in the `DaemonCommand` handler (e.g., in `start_daemon` or `restart_daemon` before spawning the daemon). The `url` crate is likely already a transitive dependency.
- **Scope**: UX, operability

#### F-107: `reqwest` as unconditional dependency in `nexus-orchestration`

- **Severity**: Low
- **Source**: `crates/nexus-orchestration/Cargo.toml` — `reqwest = { workspace = true }`
- **Observation**: `reqwest` is added as a **hard, unconditional** dependency to `nexus-orchestration`, even though the CDN network path is purely optional. Every binary that links `nexus-orchestration` (including builds without `--cdn-url` support) pays the compile time, binary size (~200KB), and dependency audit cost of `reqwest`.
- **Impact**: The `reqwest` workspace dependency was pre-existing (used by other crates), so this does not add a new crate to the workspace. However, it links `nexus-orchestration` to tokio's full async HTTP stack unconditionally.
- **Recommendation**: Gate `reqwest` behind a Cargo feature flag (e.g., `registry-cdn`) in `nexus-orchestration/Cargo.toml`, and only enable it from `nexus42`/`nexus-daemon-runtime` where the CDN CLI flag exists. Acceptable for V1.56 given pre-existing workspace dep; register as residual.
- **Scope**: Dependency hygiene, binary size

#### F-108: `count_capabilities` uses heuristics for CDN response parsing

- **Severity**: Low
- **Source**: `crates/nexus-orchestration/src/capability/builtins/registry.rs` — `count_capabilities()`
- **Observation**: The function tries `capabilities` → `agents` → `items` → top-level key count as a heuristic to count capabilities from an undefined CDN response schema. This is fragile: if the CDN returns a response with, say, a `capabilities` wrapper object rather than array, the count will be incorrect.
- **Impact**: The `capability_count` field in CDN-mode responses may be inaccurate. The CDN's response format is not yet specified, making this heuristic a placeholder.
- **Recommendation**: Define and document the expected CDN response format in a spec (e.g., `{"capabilities": [...]}`), enforce it in `count_capabilities`, and reject unexpected shapes. Document this as a "CDN response contract" in `acp-capability-set.md` §4.7A.
- **Scope**: API contract, correctness

#### F-109: Schema/contract boundary — RegistryRefreshOutput changes may affect P3 (DF-56)

- **Severity**: Low
- **Source**: Cross-cutting — `RegistryRefreshOutput` struct vs planned P3 consumption
- **Observation**: P3 (track B, Wave 3) plans to consume DF-29's `registry.refresh` output for conditional routing edges. The output shape is now locked in `RegistryRefreshOutput` with `capability_count`, `snapshot_version`, `source`, etc. If P3 needs additional fields (e.g., per-capability metadata, freshness indicators), this may require another schema change that could have been anticipated.
- **Impact**: Low — the current output is sufficient for a capability-count-driven conditional edge. If P3 needs richer data, it can extend the struct in its own scope.
- **Recommendation**: Flag for P3 planning: P3 should explicitly state which `RegistryRefreshOutput` fields it consumes, and whether the current schema is sufficient or needs extension. This is coordination advice, not a finding in P1's code.
- **Scope**: Cross-plan coordination, API stability

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| F-101 | Manual code review | `mod.rs` RegistryRefreshOutput struct | High |
| F-102 | Manual code review | `registry.rs` static CDN_CONFIG | High |
| F-103 | Manual code review | `host_tool_executor.rs` execute_registry_refresh | High |
| F-104 | Manual code review | `capability_registry.rs` request_schema_ref | High |
| F-105 | Manual code review | `registry.rs` REGISTRY_SNAPSHOT_CAPABILITIES vs `acp-capability-set.md` | High |
| F-106 | Manual code review | `daemon/mod.rs` --cdn-url flag definition | Medium |
| F-107 | Manual code review | `nexus-orchestration/Cargo.toml` + registry.rs | High |
| F-108 | Manual code review | `registry.rs` count_capabilities() | High |
| F-109 | Manual reasoning | Cross-plan — P3 dependency on P1 output schema | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 6 |

### Positive Observations

These areas meet or exceed architecture/maintainability expectations:

1. **Clean scope boundary**: P1 stays entirely within its defined §Scope In. No scope creep into DF-31/DF-42 (P0) or DF-56 (P2/P3). The implementation touches only registry.refresh-specific files and spec amendments.

2. **Capability registration follows established pattern**: The new `nexus.registry.refresh` entry in `capability_registry.rs` mirrors the existing 19 entries (same `CapabilityRow` struct, same `Access::Read` + `FailureMode` pattern). The allowlist in `host_tool_executor.rs` and the entity-count test update (19→20) are consistent with prior convention.

3. **Spec amendments are internally consistent**: Both `acp-capability-set.md` §4.7A and `cli-spec.md` §6.3 are well-integrated — they match the implementation semantics and follow the existing document structure. The `cli-spec.md` amendment correctly documents that timeout/retry are not individually configurable via CLI (deferred to post-V1.56).

4. **Feature flag propagation is complete**: `--cdn-url` flows through all three entry points (`daemon start`, `daemon restart`, `__internal daemon-run`) without loss — verified in the diff. The restart handler correctly passes the flag to the child process.

5. **Boot ordering is correct**: `set_cdn_config()` is called in `boot.rs` before capability registry construction and `WorkspaceState::initialize()`. The lazy-fetch design (first `registry.refresh` call, not daemon start) prevents startup delay.

6. **Embedded snapshot is self-validating**: Tests verify no duplicates, all IDs have `nexus.*` prefix, golden version stability, and deterministic synthetic output. These guardrails will catch snapshot drift during maintenance.

7. **File move awareness**: The `host_tool_executor.rs` file was moved from `nexus-acp-host` to `nexus-daemon-runtime` by P0; P1 correctly modified the new path.

8. **Dependency addition is minimal**: Only `reqwest` added at the crate level (was already a workspace dependency). `chrono` and `serial_test` were pre-existing.

### Risk Assessment

| Risk from plan | QC1 Assessment |
|----------------|----------------|
| Synthetic snapshot goes stale between releases | LOW — snapshot version is pinned; `cdn` path provides freshness; version-stability test guards against accidental drift |
| Network timeout/retry blocks daemon startup | CONFIRMED RESOLVED — fetch is lazy (first `registry.refresh` call triggers it), not at boot (matches `boot.rs` implementation) |
| Capability ID collides with existing IDs | CONFIRMED RESOLVED — `nexus.registry.refresh` is unique in capability_registry.rs (20 tools) and builtins registry (24 + 0 new builtins) |
| Embedded snapshot inflates binary size | LOW — 31 compile-time string constants (~1 KB); no full registry metadata; acceptable |

**Verdict**: **Approve with comments**

No critical or mandatory high-severity architectural issues. The three Warning findings (F-101: breaking output schema rename, F-102: global mutable state, F-103: force parameter ignored) are medium-severity concerns — none is a blocker for merging. All eight acceptance criteria from the plan are met by the implementation. The implementation is safe to merge to `iteration/v1.56` for mid-QA.

PM may register residual findings for F-102 (global state refactor), F-107 (feature gate reqwest), and F-105 (snapshot drift) as post-V1.56 technical debt.
