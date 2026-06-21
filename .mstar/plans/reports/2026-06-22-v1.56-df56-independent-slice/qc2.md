---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-22-v1.56-df56-independent-slice"
verdict: "Request Changes"
generated_at: "2026-06-22"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2 (Reviewer #2 — Security and correctness risk)
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk (expression grammar security, merge/converge semantics, DAG enforcement, runtime error handling, injection/DoS surfaces)
- Report Timestamp: 2026-06-22

## Scope
- plan_id: `2026-06-22-v1.56-df56-independent-slice`
- Review range / Diff basis: `a457a8ee..4da874db`
- Working branch (verified): `iteration/v1.56`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 7 (1420 insertions, 32 deletions)
- Commit range: `a457a8ee`..`4da874db` (feature `ee678812` + merge `4da874db`)
- Tools run: git diff analysis, source reading of expr.rs / tasks/mod.rs / loader.rs / validation.rs / contracts, cargo test -p nexus-orchestration (observed 1 failure), cargo clippy -p nexus-orchestration -p nexus-contracts -- -D warnings (clean), cargo +nightly fmt --all --check (clean), cross-reference with plan ACs and `.mstar/knowledge/specs/preset-conditional-routing.md` §3.3

## Findings

### 🔴 Critical

None.

### 🟡 Warning

#### W-001: Stale integration test expects V1.42-era rejection (HIGH — CI gate violation)

**Summary**: Test `reject_conditional_next_not_yet_supported` in `crates/nexus-orchestration/tests/preset_validation.rs:107-142` still calls `unwrap_err()` expecting `ConditionalNotYetSupported`, but P2 intentionally accepts conditional `next` (both legacy `kind: conditional` and new `branches` form) on arbitrary state kinds. The test now panics on `Ok(...)`.

**Evidence** (reproduced):
```
thread 'reject_conditional_next_not_yet_supported' panicked at .../preset_validation.rs:134:71:
called `Result::unwrap_err()` on an `Ok` value: LoadedPreset { id: "cond-test", ... }
```

**Source**: `tests/preset_validation.rs:107` (integration test file). Loader unit tests in `loader.rs` were updated to expect success; this parallel integration test was missed.

**Impact**: `cargo test -p nexus-orchestration` fails. Per `mstar-review-qc` CI gate rules, any related CI failure is treated as >= Warning and blocks Approve until resolved.

**Fix**: Update the test to assert `is_ok()` (and optionally validate that targets/default are accepted), mirroring the pattern already applied in `loader.rs` tests (`reject_conditional_next`, `expression_conditional_still_rejected`).

---

#### W-002: `converge` (merge-point) types defined but runtime semantics completely unimplemented (HIGH — AC4 partial)

**Summary**: `ConvergeConfig` + `ConvergeStrategy` (`wait_for_all`, `first_completed`, `any`) are added to the wire contract (`nexus-contracts/src/local/orchestration/preset.rs`) and `StateDefinition.converge`, and the spec (`preset-conditional-routing.md` §3.3.3) documents dedicated converge nodes. However:

- `StateCompositeTask::from_manifest` only copies `state.merge`, never `state.converge`.
- `resolve_expression_target` (and the surrounding run path) performs direct `NextAction::GoTo(target)` or `GoTo(default)` with **zero arrival counting, no WaitForInput re-entry, no cancellation, no strategy dispatch**.
- Loader `build_outer_graph` / `build_wired_outer_graph` still only skips wiring for `Conditional`/`Branches` (no converge-specific edges or fan-in tracking).
- No converge validation in `validation.rs` (contrast with `check_merge_node_integrity` for V1.52 labeled `merge:`).
- No runtime tests for converge strategies.

**Source Trace**:
- Contracts: `ConvergeConfig`, `ConvergeStrategy` (lines ~446-497)
- StateDefinition: `pub converge: Option<ConvergeConfig>` (line 218)
- Tasks: `resolve_expression_target` (lines 851-909) — pure expression dispatch, no converge gate
- Loader: conditional edges left unwired (same as before)
- Validation: only reachability extended; no converge integrity check

**Impact**: Acceptance criterion 4 ("Merge points accept multiple incoming edges; wait-for-all and first-arrival semantics are configurable and tested") is **not met at runtime**. A preset declaring `converge: { strategy: wait_for_all }` will advance on the *first* branch arrival; other branches may produce duplicate work or incorrect join state. This is a correctness violation of the declared contract and a latent source of non-deterministic or lost work under concurrent branches.

**Relation to prior work**: V1.52 T-B P1 implemented merge semantics for *labeled* edges via `merge:`, `expected_incoming`, `_merge_<id>` context keys, and `WaitForInput` re-entry. The new `converge:` form for expression-based branches duplicates the concept but does not reuse or extend that machinery.

**Fix**: Either (a) implement converge tracking (adapt the V1.52 pattern or a fresh one) + validation + tests before claiming AC4, or (b) explicitly defer converge runtime to P3 / a later plan, remove `converge` from the P2 schema delta, and document the gap as a residual with a durable roadmap entry. Do not leave a parsed-but-ignored field in a shipped slice.

---

#### W-003: Expression parser has no nesting / recursion depth limit — stack overflow DoS from user-installable presets (HIGH — security / availability)

**Summary**: The hand-written recursive-descent parser in `crates/nexus-orchestration/src/preset/expr.rs` (`parse_or_expr`, `parse_and_expr`, `parse_unary`, `parse_primary`, `parse_field_access`) has **no depth counter or limit**. A preset author can supply a `when:` expression with thousands of nested parentheses (e.g. `((((((_context.x == 1))))))))` or a long chain of `&&` / `||`.

- Tokenization is iterative (safe).
- Parsing recurses per nesting level / operator precedence.
- Evaluation also recurses (with short-circuit for `&&`/`||`).

**Attack surface**:
- User-installed presets live in `~/.nexus42/presets/<id>/` and are discovered and loaded by the daemon at scan time or on first use (`scan_user_presets`, `load_preset_from_str_with_limits` only limits YAML *size* and *nesting depth of YAML structure*, not expression AST depth).
- Expressions are parsed at runtime inside `resolve_expression_target` on every execution of a state that has conditional branches (not just at load).
- A malicious or accidentally-deep expression can overflow the Rust thread stack during `parse()` (at load or first hit) or during `evaluate()`.

**Evidence in code**:
- No `depth` parameter or `MAX_EXPR_DEPTH` constant.
- `parse()` and `evaluate()` are plain recursive functions.
- `DEFAULT_MAX_YAML_DEPTH` (loader) protects YAML structure but expressions are opaque strings inside `when:`.

**Impact**: Availability / reliability. In the worst case, a crafted preset can crash the orchestration task thread (or, if the engine uses a small thread pool, degrade the daemon). Because preset YAML is the source of "when" strings, and user presets are intentionally supported, this is untrusted input reaching a recursive evaluator without bounding.

**Fix**:
1. Add a recursion depth limit (e.g. 64 or 128) in the parser and evaluator; return a clean `ExprError::Parse` / `TypeError` (or a new `ExprTooDeep`) on violation.
2. Consider parsing expressions once at preset load/validation time and caching the AST (or at least the validated form) instead of re-parsing on every state execution.
3. Add a fuzzer or property test that generates deep expressions and asserts graceful error (not panic).

This is distinct from W-003 in qc1 (null semantics). This is a new resource-exhaustion / crash vector.

---

#### W-004: No validation or runtime accounting for `converge` fan-in (MEDIUM — correctness / orphan merge points)

**Summary**: Unlike `merge:` nodes for labeled edges (validated in `check_merge_node_integrity` for ≥2 incoming, quorum N/M bounds, and outgoing edge requirement), states declaring `converge:` receive:
- No check that they have ≥2 incoming conditional/Branches edges.
- No pre-computation of expected incoming count (contrast with `expected_incoming` for labeled merges).
- No warning/error for "orphan converge" (0 or 1 predecessor) or for converge nodes that are not reachable via any conditional path.

**Source**: `validation.rs` — only the reachability BFS was extended for Conditional/Branches targets; `check_merge_node_integrity` and loader merge validation are unchanged and do not inspect `converge`.

**Impact**: Authors can declare a `converge` strategy on a state that will never see multiple arrivals, or on a node with insufficient predecessors. Because the runtime currently ignores `converge` entirely, the symptom is silent (the node just acts as a normal state). Once converge runtime is added, these degenerate cases could cause hangs (wait_for_all with 1 predecessor) or surprising "first arrival" behavior.

**Fix**: Extend `check_merge_node_integrity` (or add `check_converge_integrity`) to also count incoming conditional edges (from both `Conditional` and `Branches` forms) and apply analogous rules when `converge` is present. At minimum emit a Warning diagnostic for orphan converge nodes.

---

#### W-005: `build_context_json` whitelist is too narrow; expressions cannot observe most context (MEDIUM — correctness)

**Summary**: `resolve_expression_target` builds a JSON context for the expression evaluator from a hardcoded list of 9 keys only:
```rust
let known_keys = [
    "_judge_result", "_judge_reason", "_judge_label", "_state_result",
    "_run_id", "output", "result", "status", "score",
];
```

Any value written by `context_update` hooks, output bindings, or prior states under custom keys (or nested under `state.*` paths) will be invisible to `_context.*` expressions — they resolve as `null`.

**Source**: `tasks/mod.rs:916-935` (`build_context_json`).

**Impact**: Authors following the spec ("field comparisons on context state, stage output, and work metadata") will write expressions that silently take the `default` branch because the referenced fields are never present in the JSON passed to `evaluate`. This is a functionality gap, not a security injection, but it undermines the value of the expression routing feature.

**Relation to qc1**: Same as S-001 in qc1.md. Elevating to Warning here because it directly affects correctness of the new routing primitive.

**Fix options**:
- Serialize the full `graph_flow::Context` (it implements Serialize) and let expressions see everything.
- Or document the whitelist explicitly in the spec and in error messages when a bare field is missing.
- Prefer full serialization with a size/depth guard if needed.

---

### 🟢 Suggestion

#### S-001: Re-parse of expressions on every state execution (perf / correctness hygiene)

`resolve_expression_target` calls `crate::preset::expr::parse(&rule.when)` inside the hot loop for every branch on every execution of the state. Parse errors are logged at warn level and the branch is skipped.

**Recommendation**: Parse (or at least validate) expressions once during preset load / `StateCompositeTask` construction and store `Vec<(Expr, target)>` (or keep the string + a cached `Result<Expr, _>`). This removes repeated work and makes parse errors fail at load time (clearer to the preset author) rather than at runtime (silent fallthrough to default + noisy logs).

#### S-002: `NextTarget::Conditional` vs `Branches` duality adds surface area

See qc1 S-002. Both forms end up using the same `ConditionalRule` shape and identical dispatch in loader/validator/runtime. Consider a post-P2 cleanup that normalizes legacy `kind: conditional` YAML into the `branches` form at deserialization time (or deprecate the `Conditional` variant).

#### S-003: String comparison uses raw `==` (Unicode codepoint equality)

`json_eq` for strings does direct `a == b`. No NFC/NFD normalization, no case-folding. For routing control labels authored in presets this is usually fine, but if expressions ever compare user-supplied natural-language values, authors may be surprised by "é" != "e\u{0301}".

**Recommendation**: Add a short note in the spec (under Expression grammar) that string equality is byte-for-codepoint and recommend authors normalize upstream if they need linguistic equality.

#### S-004: Error messages include the full expression string

`ExprError::Parse` / `UnexpectedEnd` embed `full_expr`. Currently this only appears in tracing logs when a preset expression is bad. If in the future expressions could be supplied from less-trusted runtime context, this would be a minor information disclosure. For now it is acceptable (presets are the source), but worth a one-line comment.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|---|---|---|---|
| W-001 | test failure + code review | `tests/preset_validation.rs:134` vs updated loader tests + P2 scope | High |
| W-002 | diff + runtime path audit | `ConvergeConfig` in contracts + absence in `from_manifest`, `resolve_expression_target`, loader graph builders, validation | High |
| W-003 | static analysis of parser | `expr.rs` recursive descent (`parse_or_expr` etc.) with no depth counter; called from runtime `resolve_expression_target` | High |
| W-004 | validation surface review | `check_merge_node_integrity` only covers labeled `merge:`; no equivalent for `converge:`; reachability only extended for targets | High |
| W-005 | code review of context assembly | `tasks/mod.rs:916` `build_context_json` whitelist of 9 keys only | High |
| S-001 | runtime path review | `resolve_expression_target` re-calls `parse()` per branch per execution | Medium |
| S-002 | schema diff | `NextTarget` enum carrying both `Conditional(NextConditional)` and `Branches(ConditionalBranches)` | Medium |
| S-003 | evaluator | `json_eq` string arm does direct `==` | Low |
| S-004 | error type | `ExprError` Display includes `full_expr` | Low |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 5 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

## Security & Correctness Assessment (qc-specialist-2 focus)

### Expression grammar security (per dispatch checklist)

- **Injection risk**: None. The grammar is a tiny arithmetic/boolean language with no side effects, no I/O, no function calls, no loops, and no host-language escape. Expressions come from preset YAML (loaded under size/depth YAML limits) and are evaluated in-process against a JSON value. Not a code-execution or SSRF sink.
- **Field path safety**: `resolve_field` returns `&Null` for any missing segment; never panics. Safe.
- **Type safety**: Incompatible comparisons (string > number) produce `ExprError::TypeError` (clean, typed). Numeric ops require both sides convertible to f64 or error. Good.
- **String comparison**: Raw `String ==`. Unicode codepoint equality. No normalization. Low-risk for control labels; document as suggestion.
- **Resource exhaustion**: **HIGH**. No recursion depth limit on parser or evaluator. Deeply nested expressions from user presets can overflow the thread stack during load or first execution of the state. This is the most material new security/correctness finding in the P2 delta.

### Merge / converge correctness

- Labeled-edge merge (`merge:` from V1.52) remains intact and is not regressed.
- New `converge:` form for expression-based branches is **parsed and stored but never acted upon**. This is a direct violation of the "wait-for-all / first-arrival semantics are configurable and tested" acceptance criterion.
- No arrival tracking, no `WaitForInput` re-entry, no cancellation of sibling branches, no idempotency for `any`. First arrival always wins today.
- Edge cases (1 predecessor, 0 predecessors, 100+ predecessors) are not validated and would behave incorrectly once implemented.

### DAG enforcement

- Reachability BFS in `check_initial_to_terminal_reachability` was correctly extended to traverse `Conditional` rules + default and `Branches` rules + default. All declared targets are now considered reachable.
- No new cycle detection was added for conditional + converge topologies. The existing inner-graph `detect_cycle` is unchanged. Because the outer graph reachability is a BFS that builds an adjacency list from declared edges, a true cycle in the conditional graph would manifest as non-termination or duplicate visitation in the reachability check (current implementation does not appear to have an explicit "visited" set that would detect it for outer graphs in the same way inner graphs do). However, the plan explicitly allows "cycles through merge points with `wait_for_all` and acyclic predecessors" as a modeling pattern; the current validator does not attempt to distinguish those. This is acceptable for P2 given that converge is not yet wired.

### Runtime correctness

- `resolve_expression_target`:
  - Handles missing fields as `null` (per `resolve_field`).
  - Empty / malformed context: the whitelist simply omits keys; evaluation proceeds with Nulls.
  - Parse/eval errors are logged at warn and the branch is skipped (graceful, but can lead to surprising default routing and log spam).
  - Falls back to `default` (which the loader has already validated exists).
- No obvious panic paths or unwraps on untrusted data in the expression path.
- Concurrent context update races: the context snapshot is taken at task entry; expression evaluation is synchronous within the task. Graph_flow session model appears to serialize task execution per graph instance, so this is not a live data race under normal operation.
- Error messages do not leak sensitive runtime data (they include the expression from the preset, which is author-controlled).

### Other

- No new "user-controlled string → network/eval sink" surface that would trigger the V1.56 P1 SSRF precedent. The eval is purely local and side-effect-free.
- Backward compatibility for V1.42 GoNogo and V1.52 Labeled paths is preserved (65+ existing tests continue to pass).

## Positive Findings

1. Expression grammar implementation is clean, well-tested (37 parser/evaluator tests), and appropriately minimal. No feature creep into a full scripting language.
2. Reachability validator was updated in the right place (`check_initial_to_terminal_reachability`) to account for the new conditional forms.
3. Loader validation now accepts conditional next on any state kind (the core scope of P2) and correctly validates that all branch targets + default refer to declared state IDs.
4. Clippy (`-D warnings`) and nightly fmt are clean across touched crates.
5. No regression in the labeled merge path or GoNogo path.

## Verdict Rationale

**Request Changes** is required for the following reasons (any one would be sufficient under the gate rules):

1. **W-001** is a hard CI failure on `cargo test -p nexus-orchestration`. Per `mstar-review-qc`, related CI failures are >= Warning and block Approve.
2. **W-002** means a core acceptance criterion (AC4 — merge point semantics) is not delivered at runtime. The types and spec text exist; the behavior does not. This is a correctness gap, not a documentation gap.
3. **W-003** is a new security/availability finding (stack overflow from untrusted preset expressions) that must be bounded before the slice is considered safe.

W-004 and W-005 are additional correctness issues that should be addressed or explicitly residualized.

Once the stale test is fixed, converge is either implemented or clearly deferred with a residual + roadmap entry, and the parser is hardened against deep recursion, the change is architecturally sound and the security surface is acceptable for a local expression language.

## Plan Update

2026-06-22-v1.56-df56-independent-slice (qc2 done; awaiting qc3 + PM consolidate of qc1+qc2+qc3)

## Handoff

PM should consolidate qc1 + qc2 + qc3 into `qc-consolidated.md`, decide on targeted re-review vs full tri re-review, and either:
- Dispatch a fix-wave for W-001 + W-002 + W-003 (recommended), or
- Record explicit residual + "accept with known gaps" decision for converge and parser depth before moving to mid-QA.

**Git** (to be executed after writing this report):
- Working branch: `iteration/v1.56`
- Reviewed range: `a457a8ee..4da874db`
- No implementation changes; only this QC report committed.
