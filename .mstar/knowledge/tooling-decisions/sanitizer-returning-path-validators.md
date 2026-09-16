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

returns only a verdict. It checks `id` and returns `Ok(())` — the *same* tainted `&str` then flows on, unchanged, into `Path::join`. The checker has no way to know the earlier call eliminated the hazard, so it reports path injection at every later join.

This surfaced on `nexus_home_layout::validate_entry_id_safe` in v1.190: the function is correct (it rejects empty ids, `/`, `\`, `..` and control characters, and is unit-tested for each), and the callers are correctly guarded, but the analysis still flagged the KB entry path construction because the tainted value was never rebind to a sanitized result. The finding is a modelling gap, not a vulnerability.

The tension is real: the project's own rule is "do not silently suppress symptoms", but this is a case where the *code shape* is the only thing the checker can read — dismissing the alert in the UI fixes the report without fixing the model, and the next tainted id on a new path reintroduces it.

## Guidance

**Prefer the shape the analyzer can model.** Return the sanitized value rather than a verdict:

```rust
/// Validate and return an id that is safe to use as a single path component.
///
/// # Errors
/// Returns `Err` if the id is empty, contains `/`, `\`, `..`, or control chars.
pub fn sanitize_entry_id(id: &str) -> std::result::Result<String, String> {
    if id.is_empty() { return Err("entry_id must not be empty".to_string()); }
    if id.contains('/') || id.contains('\\') { return Err(/* … */); }
    if id.contains("..") { return Err(/* … */); }
    if id.chars().any(char::is_control) { return Err(/* … */); }
    Ok(id.to_string())
}
```

Callers then rebind, and every later `join` consumes a value that came out of the validator:

```rust
let entry_id = sanitize_entry_id(&raw_id).map_err(|reason| invalid_input("entry_id", reason))?;
let dest = entries_dir.join(format!("{entry_id}.md"));  // entry_id is post-validator
```

Two independent reasons to make this the default:

1. **Taint modelling.** The returned value is the sanitizer's output, so the analyzer stops propagating the original taint. No alert, no UI dismissal, no re-alert on the next new path site.
2. **Type-level enforcement.** `Result<String, _>` makes it *impossible* to use the raw id after validation without an explicit re-bind. The `Result<(), _>` shape lets a caller ignore the return (`let _ = validate(…)`, or a bare `?` on a copy) and still use the original binding — the compiler does not object. That is a real, if latent, bug class the return-value form removes.

**When the return-value form is impractical**, the fallback is an explicit, documented dismissal — but treat it as debt with an owner and a target, not as a resolution:

- Record the modelling gap and the intended restructure in the roadmap (so it is not lost).
- Dismiss the specific alert with a justification that names the validating function and says "false positive; sanitizer modelling tracked".
- Do **not** add a blanket analyzer exclusion for the rule — that would hide genuine path-injection findings in the same tree.

**Do not** "fix" it by inlining a redundant re-check next to every `join`, or by prefix-matching the string, to make the analyzer happy. Those add code that does not strengthen the real guarantee and drift from the canonical validator.

## Why This Matters

- **Re-alerting is the real cost.** An alert dismissed in the GitHub UI is invisible to the next person who adds a path; a sanitizer-shaped validator keeps the whole family clean for every future caller at zero marginal cost.
- **The two shapes have different safety, not just different syntax.** `Result<(), _>` validation-only leaves the tainted binding available; `Result<String, _>` forces the safe value to be used. The second is what the analyzer models *and* what removes the footgun.
- **Suppression discipline.** A blanket rule exclusion would be the cheapest-looking fix and the most damaging: it converts a known false positive into an unknown set of true negatives.

## When to Apply

- Any `validate_*_safe` / `check_*` helper whose argument later flows into a filesystem path (or another taint sink like a shell command or SQL identifier) and which static analysis reports.
- Designing a new path-confinement helper: return the confined value (`PathBuf`, `String`), never a bare verdict.
- Triaging a static-analysis alert that is genuinely a false positive: check whether reshaping the code removes it before reaching for a dismissal, and if you dismiss, attach an owner and a roadmap entry.

## Examples

### Before — verdict-only validator, taint survives

```rust
pub fn validate_entry_id_safe(id: &str) -> std::result::Result<(), String> { /* checks */ }

// caller
validate_entry_id_safe(&entry_id)?;
let candidate = entries_dir.join(format!("{entry_id}.md"));  // analyzer: tainted -> path-injection
```

### After — sanitizer returns the safe value

```rust
pub fn sanitize_entry_id(id: &str) -> std::result::Result<String, String> { /* checks */ }

// caller
let entry_id = sanitize_entry_id(&raw_id)?;
let candidate = entries_dir.join(format!("{entry_id}.md"));  // post-sanitizer value
```

The migration is deliberately call-site-affecting: every existing caller rebinds, which is exactly the moment to confirm the validated value is the one used in the path (a call site that validated id A and joined id B becomes visible).

## Evidence

- The v1.190 instance — `crates/nexus-home-layout/src/lib.rs` `validate_entry_id_safe` (rejects empty, `/`, `\`, `..`, control chars; six unit tests, one per rejection class + an accepting case). Callers in `crates/nexus-core/src/knowledge.rs` (`get_kb_entry`, `delete_kb_entry`) run it before path construction, and the CLI surface in `apps/nexus42/src/commands/creator/kb.rs` does the same through `paths::validate_entry_id_safe`.
- Reported sink — `rust/path-injection` on the `entry_id`-derived entry path in `knowledge.rs`, where the id reaches `entries_dir.join(format!("{entry_id}.md"))` for read, write and delete.
- Recorded disposition — the roadmap direction entry for v1.190 tracks the restructure to a sanitized-value return, and directs dismissing the existing alert with a justification naming the validator, so both the alert and the intent are visible to the next reader.
- Related convention — [canonical-invalid-input-422.md](../engineering-conventions/canonical-invalid-input-422.md) for how these validators' errors are mapped onto the wire.
