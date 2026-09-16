---
module: nexus-core + nexus-daemon-runtime (ActorSessionRegistry transitional duplicate)
date: 2026-09-17
problem_type: build_error
category: build-errors
severity: high
plan_id: 2026-09-15-v1.190-p4-provider-host-control-ports
symptoms:
  - "a whole-tree text rewrite of one registry method's call shape produces E0308 'expected Snapshot, found &Snapshot' at every call site in the other crate"
  - "the rewrite compiles clean in the crate that owns the changed signature and fails only in the crate that still has the old one"
  - "grep for the symbol returns definitions in two crates, so a match count cannot tell which signature a hit targets"
root_cause: a migration left the same-named type (ActorSessionRegistry, CharacterOperationSnapshot) and the same-named method in both the new nexus-core authority and the not-yet-rewired nexus-daemon-runtime shim; the two definitions had already diverged in parameter mode and error type, so any cross-crate text-level edit is applied to two incompatible signatures at once
resolution_type: code_fix
tags:
  - duplicate-symbol
  - transitional-crate
  - cross-crate-migration
  - blanket-rewrite
  - e0308
  - rust
---

# Same-named registry methods in two crates break cross-crate text rewrites

> Provenance: the duplicate definitions and their signature divergence are observed at HEAD; the failing rewrite is reproduced on a two-crate fixture. The analogous recorded incident in this repo is the `open_direct_core` → `open_write_core` rename (four callers, reverted before commit) described in Prevention.

## Problem

During the v1.190 core extraction, `ActorSessionRegistry` was **copied** into `nexus-core` while the daemon's original stayed in place (the daemon transport was deliberately not rewired in that task, so both definitions had to compile). The two copies then diverged: a later lint pass changed `nexus-core`'s `reserve_character_operation` to take a reference, while the daemon copy kept taking the snapshot by value.

That divergence makes a symbol-level text rewrite dangerous. Rewriting the call shape across the tree (`reserve_character_operation(snapshot.clone())` → `reserve_character_operation(&snapshot)`) is correct for the core crate and wrong for every daemon call site; the edit applies to both because a grep for the method name returns both.

The hazard is structural, not incidental: 13 by-value call sites live in the daemon crate and one by-reference site in core, and the two crates expose the same method name over same-named-but-distinct types. Any tree-wide rewrite of that call shape therefore has to be split per crate, and nothing in a grep reports which crate a hit belongs to.

## Symptoms

- A tree-wide rewrite of one call shape produces many `E0308` mismatched-types errors at the call sites belonging to the *other* crate:

  ```
  error[E0308]: mismatched types
     --> crates/registry-shim/src/lib.rs:9:15
      |   ---- ^^ expected `Snapshot`, found `&Snapshot`
  ```

- The same edit is green in the crate whose signature changed and red only in the crate that still holds the old one.
- `grep -rn "fn reserve_character_operation" crates/` returns **two** definitions, in different crates, with different signatures — so neither the hit count nor the call-site count identifies which contract a given line targets.
- Reverting is trivial but the error list looks like a real compile regression, inviting a "fix the call sites" round that would then break the *other* crate.

## What Didn't Work

- **A single mechanical rewrite across every `.rs` file** for the method: it would change the 13 by-value call sites in `actor_run_capture.rs` (which belong to the daemon's by-value copy) together with the core's by-reference site, making the daemon crate uncompilable in one pass. The rewrite must be scoped to the crate that owns the changed signature. Reproduced in a two-crate fixture: applying the reference form to the by-value crate yields one `E0308` per call site, all `expected Snapshot, found &Snapshot`.
- **Counting call sites to estimate blast radius.** The count is meaningless until each hit is attributed to a crate, because both crates expose the same method name over the same-named type.
- **Treating the duplicate as a rename target.** The two types are not aliases — `CharacterOperationSnapshot` in core and in the daemon are distinct structs, and the error types differ too (`CoreResult<()>` vs `Result<(), NexusApiError>`), so a textual signature sync in either direction leaves one side wrong.
- **Assuming a commit message's call-site count describes the tree.** The core-lib lint commit that introduced the by-reference form changed exactly one core call site in its diff (`snap.clone()` → `&snap` in `host.rs`) while its message claims "net: 13 call sites stop cloning a CharacterOperationSnapshot" — the 13 are the daemon sites that clone, and they still clone at HEAD because the daemon copy still takes by value. Read the diff, not the summary, when estimating cross-crate blast radius.
- **Reverting and moving on.** `git checkout HEAD --` over the affected paths restores the tree, but the next edit to either copy re-hits the same trap unless the call-site ownership is established.

## Solution

Attribute every hit to its owning crate **before** editing, and scope the rewrite per crate:

```sh
# 1. Both definitions, with their real signatures — this is the divergence evidence
grep -rn "fn reserve_character_operation" crates/ --include=*.rs

#   crates/nexus-core/src/actor_sessions.rs:309            snapshot: &CharacterOperationSnapshot  -> CoreResult<()>
#   crates/nexus-daemon-runtime/src/workspace/actor_sessions.rs:385  snapshot: CharacterOperationSnapshot -> Result<(), NexusApiError>

# 2. Call sites, grouped by crate — count per crate, not in total
grep -rn "reserve_character_operation" crates/ apps/ --include=*.rs \
  | grep -v "fn reserve_character_operation" \
  | awk -F: '{print $1}' | sort | uniq -c | sort -rn

# 3. Rewrite only the crate the signature change belongs to
#    (core: `&snapshot`; daemon: leave the by-value form alone)
```

The durable fix is to stop having two copies: the duplicate is transitional state, and its second owner must be scheduled away. Until then, keep the divergence **visible at the definition** (core's by-reference form is plainly readable in the source, and the daemon's keeps its `#[allow(clippy::needless_pass_by_value)] // snapshot is stored by value in the operation map` with the by-value parameter), so a reader who greps the symbol sees two different contracts immediately.

## Why This Works

A text rewrite is a **global** operation while a signature is a **per-definition** contract. As long as two crates define the same method name over same-named (but distinct) types, no symbol-level tool can infer the intended target from the call shape alone — the call site's receiver type is what decides, and that is exactly the information a grep drops.

Scoping by crate restores the missing dimension: the definition's own crate tells you which contract applies. Verified in a two-crate fixture — one crate with the by-reference signature and one with by-value, same type name and method name: the by-value crate's call sites fail with `E0308 … expected Snapshot, found &Snapshot` the moment the reference form is applied to them, and compile clean when the rewrite is limited to the other crate.

The reason the class is worth recording rather than treating as a one-off: a partially migrated workspace *by construction* contains such duplicate families, and the window between "new authority extracted" and "old caller rewired" is exactly when routine maintenance edits (lints, signature cleanups, mechanical refactors) are most likely to run. The duplicate is what makes a normally safe rewrite unsafe.

## Prevention

- **Before a symbol-level rewrite, enumerate the definitions, not just the call sites.** Two definitions in two crates means the rewrite needs a per-crate plan; one definition means a tree-wide rewrite is safe.
- **Group call sites by owning file/crate and include the grouping in the edit plan**, so blast radius is stated per contract rather than as a single scary number.
- **Treat the duplicate itself as the defect.** Record the transitional copy with an owner and a target so the second definition is scheduled for deletion rather than discovered again. In v1.190 the daemon copy remained because the transport rewiring was explicitly deferred; the disclosure belongs in the plan, and the deletion belongs to whoever owns that rewire.
- **Do not "fix" the compile errors by editing the other crate's call sites.** That path converges on an edit war between two contracts. Revert with `git checkout HEAD -- <paths>`, re-plan per crate, then re-apply.
- **If a signature change is intended to reach both copies**, change both definitions first (and their error types), then the call sites — never the call sites alone.
- **Watch for the same class in renames, not just signature changes.** A recorded v1.190 instance: a leaf renamed `open_direct_core` → `open_write_core` in its own file; the new name broke the four existing callers (`creator/kb.rs` ×2, `creator/knowledge.rs`, `creator/works/slim.rs`, `creator/world/fork.rs`) which were owned by a different task's file set. The rename was reverted before commit and the existing name kept, so no caller file needed editing. When a symbol is shared beyond your task's writable set, prefer keeping the name over taking the rename.

## Evidence

- Duplicate definitions at HEAD — `crates/nexus-core/src/actor_sessions.rs` (`pub struct ActorSessionRegistry`, `pub struct CharacterOperationSnapshot`, `pub fn reserve_character_operation` taking `&CharacterOperationSnapshot -> CoreResult<()>`) and `crates/nexus-daemon-runtime/src/workspace/actor_sessions.rs` (same type names, `pub fn reserve_character_operation` taking `CharacterOperationSnapshot -> Result<(), NexusApiError>`). Both are compiled: `crates/nexus-daemon-runtime/src/workspace/mod.rs` declares `pub mod actor_sessions;` and the daemon handlers call `state.actor_sessions()` throughout.
- Divergence origin — the by-reference core signature was introduced by the core-lib clippy pass (`refactor(lint): resolve the core-lib clippy findings under -D warnings`). That commit's diff changes the core definition and one core call site (`crates/nexus-core/src/host.rs`, `snap.clone()` → `&snap`); its message's "net: 13 call sites stop cloning a CharacterOperationSnapshot" counts the daemon sites that clone, which were not in the diff. The daemon copy's definition dates from the earlier actor-maintenance work and was not touched by that commit.
- Failure signature reproduced — a two-crate fixture with one by-reference and one by-value definition of the same method over same-named types emits `E0308 mismatched types … expected Snapshot, found &Snapshot` at the by-value crate's call sites when the reference form is applied to them.
- Transitional-copy disclosure — the extraction task that created the core authority recorded that the daemon HTTP handlers and `workspace/actor_sessions.rs` were not rewired onto the new authority in that task, i.e. the duplicate was deliberate and its removal deferred.
