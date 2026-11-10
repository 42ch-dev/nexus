---
module: nexus-core (workspace selection, hosted execution factory) + nexus-core-node (native Host probe)
date: 2026-09-24
problem_type: architecture_pattern
category: architecture-patterns
severity: high
plan_id: 2026-09-22-v1.195-p0-workflow-control
applies_when:
  - "A runtime owner binds a resource (workspace root, home, DB) that a second plane also resolves for its own boundary"
  - "Part of the binding is PathBuf/bytes-typed and another part is String-typed, so a conversion sits between them"
  - "Deciding whether a changed selection is a moved selection or an environment fault"
  - "A readiness probe must not publish a positive answer off a fabricated or stale binding"
related_components:
  - nexus-core
  - nexus-core-node
  - nexus-wasm-host
tags:
  - canonical-root
  - open-time-pin
  - lossy-path-conversion
  - probe-owner
  - fail-closed
  - drift-classification
status: active
---

# One pinned canonical root per hosted owner — refuse what the ports cannot carry

## Context

The hosted execution owner is bound to a **creative workspace root**, and two different planes consume that binding: the native Host binds it as raw path bytes for its readiness probe, while the core-composed workspace ports (`WorkspaceCommitExecutor`, the `_context.workspace.*` state provider, `WorkspaceCommitAuthority`, the startup-recovery root filter, `RunnerDeps::workspace_root`) are `String`-typed. One owner with two roots is not a cosmetic inconsistency: the Host would probe one directory while the commit authority writes in another, and readiness would be published for a boundary the owner does not use.

Three independently verified ways that happened, each reproduced before its fix:

1. **A missing pin was silently replaced.** The Host probe resolved a selected-but-absent/invalid `local_root` into a probe owner whose workspace root was the user home, so a selected provider could be reported `provider_ready: true` with `engine_epoch: null` — a ready lane over a null owner, off a boundary the owner's own factory refuses.
2. **The selection was read twice.** Native boot read and canonicalized the selected `local_root` to decide probe ownership, and the core factory read it again after Host startup; a supported metadata write landing between the two reads probed under root A and published the execution/commit authority under root B.
3. **A lossy conversion crossed the seam.** The pin was a canonical `PathBuf` while the ports received `to_string_lossy()`; a canonical root containing non-UTF-8 bytes could therefore point the ports at a *different, possibly existing* directory (the replacement path can exist) while the Host kept probing the real bytes.

## Guidance

### 1. Pin once at admission, and hand the pin to everyone

The engine-owner admission resolves the selection **once**, canonicalizes it and keeps it as the open-time pin; the Host probe owner and the hosted factory both take the root **from that pin**, and the factory's drift check compares the current selection against the pin instead of resolving a root of its own. A second read of the same document at a different time is a second root waiting for a write to land between the reads.

### 2. Refuse, at the pin, what a port cannot carry losslessly

A canonical root that has no lossless UTF-8 form has no honest `String` form. Substituting U+FFFD silently retargets every `String`-typed port, so the correct answer is a typed environment refusal at the pin — before any probe boundary, lease, port or recovery exists — and a re-check in the factory before it builds a single port. Valid UTF-8 path bytes pass through unchanged and byte-identically, so an accepted root is the canonical root.

### 3. Classify "moved" before "unrepresentable"

The drift check and the pin ask **different** questions, and running the pin's rule first hides the case the pin exists for:

| Question | Comparison | Refusal class |
|---|---|---|
| Is the current selection still the pinned root? | raw canonical paths of the current selection vs the pin | any *different* root → the stale-admission class (a later open admits the moved root as a new epoch with its own pin) |
| Can this root be owned? | lossless UTF-8 form of the **pinned** root | no → typed environment fault |

A selection that moved to a root without a lossless form is still a *different* root: reporting an environment fault for it would hide the drift. One shared internal reader body serves both the pin and the drift comparison, so both read the same document through the same reader and resolve the same canonical form; each applies only its own rule.

### 4. No usable pin means no probe owner

An owner that cannot bind the selected root must keep the open boundary it already had and mark the selected candidates `probe_context_unavailable` — never invent a boundary (the user home is exactly that invention) and then publish readiness from it. "Probing is bound to the pinned creative root or it does not happen."

### 5. Fail closed before any side effect

Every refusal happens before the lease, the ports, the Host probe and startup recovery. The pinned root also scopes recovery: another root's unsettled intents are left untouched — not settled, rolled back, failed or deleted — because this authority was not admitted for them.

## Why This Matters

All three defects produce a **truthful-looking wrong answer**: a ready lane over a null owner, an owner writing under a different root than the one it advertises, or a probe that validates a path the commit authority never uses. The failure surface is far from the cause — the symptom appears as "readiness lied" or "the workspace effect landed somewhere else", and the cause is a second read, a replaced value or a lossy conversion in the binding seam. Making the binding a single pinned value with one representability rule removes the whole class, and it costs one refusal class per wrong input instead of a per-call-site check.

## When to Apply

- Any runtime owner that shares one selection (workspace root, home, database path, node identity) with another plane's boundary or probe.
- Any seam where a path or an identity crosses between a bytes/`PathBuf` world and a `String` world — `to_string_lossy`, `display().to_string()`, `format!` on a path.
- Designing a readiness probe: ask what the probe owner is bound to, and whether the owner itself would accept that binding.
- Deciding a refusal class for a *changed* selection versus an *unusable* one.

## Examples

### Before — the pin is converted on the way to the ports

```rust
let canonical_root: PathBuf = pin.canonical_root()?;      // raw bytes, compared for equality
let workspace_root = canonical_root.to_string_lossy();    // U+FFFD substitution is possible
// ports built from `workspace_root`; Host probe still bound to `canonical_root`
```

### After — one pin, one representability rule, one refusal point

```rust
// pin time (before any probe/lease/port exists)
lossless_root_str(&canonical_root)?;                       // typed environment refusal
// ...
// factory, before building a port
let canonical_root_str = crate::works::lossless_root_str(&canonical_root)?.to_owned();
// the pin `PathBuf` itself is what the ports that accept a path receive
```

### Before — an unusable pin is replaced

```rust
let probe_owner = selected_root.map(|root| SessionOwner { workspace_root: root })
    .or_else(|| Some(SessionOwner { workspace_root: user_home.clone() }));  // fabricated boundary
```

### After — a missing pin yields no probe owner

```rust
let probe_owner = if access == CoreAccess::EngineOwner {
    core.admission_creative_root().map(|root| SessionOwner { workspace_root: root.to_path_buf(), .. })
} else { None };
```

## Evidence

- Pin, drift comparison and the shared reader — `crates/nexus-core/src/works.rs` (`canonical_selected_workspace_root`, `selection_matches_pinned_root`, `selected_canonical_workspace_root`, `lossless_root_str`).
- Factory re-check and pin-based drift refusal — `crates/nexus-core/src/execution/production.rs` (lossless re-check before any port; the bundle composes from the pinned path).
- Probe owner from the pin only — `crates/nexus-core-node/src/lifecycle.rs` (`open_core`'s `probe_owner` block, `admission_creative_root`).
- Regressions — `a_canonical_root_without_a_lossless_utf8_form_is_refused_at_the_pin`, `a_root_without_a_lossless_utf8_form_is_refused_by_the_port_seam`, `a_moved_selection_to_a_non_utf8_root_is_refused_as_stale`, `only_the_filesystems_own_refusal_skips_the_non_utf8_fixture` (`crates/nexus-core/src/execution/production.rs`).
- Observability contract for the refusal classes — the service maps these through its existing typed error surface; no new status code is invented for them.

## Coverage gaps

- The non-UTF-8 fixture case is **Linux-only by construction**: the default development filesystem on macOS (APFS) cannot host non-UTF-8 names, and the fixture skips *only* on the measured `EILSEQ` refusal. An unrelated perturbation (for example an unexpected `EEXIST`) fails the test instead of skipping it, so a silent skip cannot pass for a green run. The representable-UTF-8 control path is covered on both filesystems.
- The negative direction — a *fresh* open that legitimately moves the selection — is covered through the stale-admission refusal; a mid-flight *external* writer that mutates the metadata document between the pin and a later read is not fault-injected.
