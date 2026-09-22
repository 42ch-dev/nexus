---
module: nexus-home-layout (path validators), nexus-core (KB entry paths)
date: 2026-09-17
problem_type: tooling_decision
category: tooling-decisions
severity: medium
plan_id: 2026-09-15-v1.190-p0-world-kb-narrative-services
applies_when:
  - a validate-then-use path guard is flagged by CodeQL rust/path-injection as a false positive
  - deciding whether to restructure a validator or dismiss a static-analysis alert
  - writing a path-confinement helper that must also satisfy taint tracking
tags:
  - codeql
  - path-injection
  - taint-tracking
  - static-analysis
  - sanitizer
  - path-validation
  - false-positive
---

# Sanitizer-returning validators for taint-tracked path guards

## Context

Static analysis with taint tracking (`CodeQL` `rust/path-injection` and equivalents) models a "sanitizer" as a function that **transforms the tainted value into a safe one**. A validator with the shape

```rust
pub fn validate_entry_id_safe(id: &str) -> std::result::Result<(), String>
```

is the **pre-v1.194 name and shape**, kept here as the record of what was reported; it no longer exists in the tree (the function is `sanitize_entry_id` today — see *Delivered shape* in Guidance and the Evidence section). It returns only a verdict: it checks `id` and returns `Ok(())`, and the *same* tainted `&str` then flows on, unchanged, into `Path::join`. As observed in v1.190, the checker had no way to know the earlier call eliminated the hazard, so it flagged path injection at the later join.

This surfaced on `nexus_home_layout::validate_entry_id_safe` in v1.190: the function is correct (it rejects empty ids, `/`, `\`, `..` and control characters, and is unit-tested for each), and the callers are correctly guarded, but the analysis still flagged the KB entry path construction because the tainted value was never rebind to a sanitized result. The finding is a modelling gap, not a vulnerability.

The tension is real: the project's own rule is "do not silently suppress symptoms", but this is a case where the *code shape* is the only thing the checker can read — dismissing the alert in the UI fixes the report without fixing the model, and the next tainted id on a new path reintroduces it.

## Guidance

**Prefer a validator that returns the value.** Return the sanitized value rather than a verdict, and have the sink consume the returned value. **Delivered shape (v1.194 P1)** — it borrows rather than allocates:

```rust
/// Validate and return an id that is safe to use as a single path component.
///
/// # Errors
/// Returns `Err` if the id is empty, contains `/`, `\`, `..`, or control chars.
pub fn sanitize_entry_id(id: &str) -> std::result::Result<&str, String> {
    if id.is_empty() { return Err("entry_id must not be empty".to_string()); }
    if id.contains('/') || id.contains('\\') { return Err(/* … */); }
    if id.contains("..") { return Err(/* … */); }
    if id.chars().any(char::is_control) { return Err(/* … */); }
    Ok(id)
}
```

Callers then bind — or shadow — the returned borrow, and every later `join` consumes a value that came out of the validator:

```rust
let entry_id = sanitize_entry_id(&entry_id).map_err(|reason| invalid_input("entry_id", reason))?;
let dest = entries_dir.join(format!("{entry_id}.md"));  // consumes the validator's result
```

Shadowing the raw binding is the call-site half of the contract: after the `let`, no name in that scope still refers to the pre-validation value. It is a **reviewable code shape, not a type-level guarantee** — see reason 1.

Two reasons to make this the default:

1. **Caller-side consumption.** The returning form hands the caller a value to use, so the sink can consume the validator's result instead of the original binding, and the shadowing convention leaves the unvalidated value without a reachable name at that call site. **A returned `String` or borrow does not by itself force any of that.** `let _ = sanitize_entry_id(&raw)?;` followed by `entries_dir.join(format!("{raw}.md"))` compiles exactly as the verdict-only form does, so a caller can still ignore the result and keep using the raw binding; the rebind/shadow at each call site is what removes the footgun, and the returning form is what makes that rebind natural to review (a call site that validated id A and joined id B becomes visible). The delivered call sites all consume the returned borrow.
2. **Taint modelling — plausible, and unverified in this repository.** Returning the sanitized value is the shape a taint-tracking checker is usually able to model, and that is why the restructure was chosen. **No analyzer recognition is claimed:** local analyzer execution is unavailable here (no `codeql` binary on PATH; CodeQL runs through GitHub default setup, not a repository workflow), the dismissed `rust/path-injection` alert predates the migration (alert `#225`, dismissed as a false positive on 2026-09-16), and no analysis of the changed revision (`12ecaa31e`) exists. The migration's evidence is unit/runtime proof, not analyzer output; analyzer evidence for the changed revision is an **open residual** at plan level.

**When the return-value form is impractical**, the fallback is an explicit, documented dismissal — but treat it as debt with an owner and a target, not as a resolution:

- Record the modelling gap and the intended restructure in the roadmap (so it is not lost).
- Dismiss the specific alert with a justification that names the validating function and says "false positive; sanitizer modelling tracked".
- Do **not** add a blanket analyzer exclusion for the rule — that would hide genuine path-injection findings in the same tree.

**Do not** "fix" it by inlining a redundant re-check next to every `join`, or by prefix-matching the string, to make the analyzer happy. Those add code that does not strengthen the real guarantee and drift from the canonical validator.

## Why This Matters

- **Re-alerting is the real cost.** An alert dismissed in the GitHub UI is invisible to the next person who adds a path; a value-returning validator is the shape meant to stop the family depending on that single dismissal for every future caller, at zero marginal cost. Read it with the caveat above: analyzer recognition is unverified here, so the restructure is justified first as code hygiene, and the alert's own disposition stays a separate tracked item.
- **The two shapes differ in what the caller is handed, and in what a reviewer can check.** Verdict-only returns nothing usable, so the same tainted binding necessarily flows on; the returning form hands back the validated value and makes the consuming call site — and any missed rebind — visible in review. Neither shape *forces* consumption: that half is the call-site convention (`let entry_id = sanitize_entry_id(&entry_id)?;`), not the signature.
- **Suppression discipline.** A blanket rule exclusion would be the cheapest-looking fix and the most damaging: it converts a known false positive into an unknown set of true negatives.

## When to Apply

- Any `validate_*_safe` / `check_*` helper whose argument later flows into a filesystem path (or another taint sink like a shell command or SQL identifier) and which static analysis reports.
- Designing a new path-confinement helper: return the confined value (`PathBuf`, `String`, or the validated borrow), never a bare verdict.
- Triaging a static-analysis alert that is genuinely a false positive: check whether reshaping the code removes it before reaching for a dismissal, and if you dismiss, attach an owner and a roadmap entry.

## Examples

### Before — verdict-only validator, taint survives (the pre-v1.194 name and shape, since replaced)

```rust
pub fn validate_entry_id_safe(id: &str) -> std::result::Result<(), String> { /* checks */ }

// caller
validate_entry_id_safe(&entry_id)?;
let candidate = entries_dir.join(format!("{entry_id}.md"));  // as reported in v1.190: tainted -> path-injection
```

### After — the delivered shape: the validator returns the value and the call site consumes it

```rust
pub fn sanitize_entry_id(id: &str) -> std::result::Result<&str, String> { /* checks; Ok(id) */ }

// caller — shadow the raw binding so no name in scope still holds the pre-validation value
let entry_id = sanitize_entry_id(&entry_id)?;
let candidate = entries_dir.join(format!("{entry_id}.md"));  // consumes the validator's result
```

The migration is deliberately call-site-affecting: every existing caller rebinds or shadows, which is exactly the moment to confirm the validated value is the one used in the path (a call site that validated id A and joined id B becomes visible). The delivered migration did that at all five call sites.

## Evidence

- **v1.190 — the reported instance (history).** `crates/nexus-home-layout/src/lib.rs` `validate_entry_id_safe` (rejects empty, `/`, `\`, `..`, control chars; six unit tests, one per rejection class + an accepting case). Callers in `crates/nexus-core/src/knowledge.rs` (`get_kb_entry`, `delete_kb_entry`) ran it before path construction, and the CLI surface in `apps/nexus42/src/commands/creator/kb.rs` did the same through `paths::validate_entry_id_safe`. That name and verdict-only shape no longer exist.
- **v1.194 P1 — the delivered restructure (commit `12ecaa31e`).** `sanitize_entry_id(&str) -> Result<&str, String>` returns the original borrow on success (`Ok(id)`), with the same rejection rules, no normalization and no allocation; the six tests are renamed (`sanitize_entry_id_returns_valid_borrow` asserts the carried borrow, the five rejection cases keep their refusal classes). All five call sites consume the borrow: `crates/nexus-core/src/knowledge.rs` binds `safe_entry_id` for the fast-path join, the slow-path entry file and the index lookup, in both `get_kb_entry` and `delete_kb_entry`; `apps/nexus42/src/paths.rs` re-exports `sanitize_entry_id` with no compatibility alias; `apps/nexus42/src/commands/creator/kb.rs` (`kb_queue_extract`) and `apps/nexus42/src/commands/acp/mod.rs` (`cmd_agent_use`, `default_agent_ref`) shadow the raw binding with the sanitizer result. Proof is runtime: `cargo test -p nexus-home-layout entry_id` (6 passed) and `crates/nexus-core/tests/reading_knowledge_services.rs::kb_scope_isolation_and_entry_round_trip` extended with an out-of-root traversal sentinel, plus a mutation round showing the guard is load-bearing (dropping the sanitizer call failed the sentinel assertion, restoring it returned green).
- Reported sink — `rust/path-injection` on the `entry_id`-derived entry path in `knowledge.rs`, where the id reaches `entries_dir.join(format!("{entry_id}.md"))` for read, write and delete.
- **Analyzer disposition — unchanged and open.** The v1.190 roadmap direction entry tracked the restructure to a sanitized-value return and the dismissal of the existing alert; the restructure was delivered in v1.194 P1, but the analyzer half is **not** closed. Alert `#225` (`rust/path-injection`, `crates/nexus-core/src/knowledge.rs:759`) was dismissed as a false positive on 2026-09-16, before this migration, and is not closure evidence for it; no CodeQL analysis of the changed revision exists and local analyzer execution is unavailable, so the taint-flow outcome for the restructured code is unestablished rather than reported-clean.
- Related convention — [canonical-invalid-input-422.md](../engineering-conventions/canonical-invalid-input-422.md) for how these validators' errors are mapped onto the wire.
