# nexus-workflow-simulate — Private Pure Simulator

The single owner of the static Simulate walk: a deterministic reachability
prediction over a candidate Workflow IR document. Nothing executes.

## Key Rules

- **Private, not SDK.** `publish = false`. No consumer may re-export it as a
  compatibility shim; callers depend on it directly.
- **One argument.** `simulate_document(&document)` takes the candidate only. The
  walk holds no registry, executor, provider, storage or engine edge, so a
  capability cannot be dispatched from here by construction, not by convention.
- **Pure.** The input document is never mutated and nothing is persisted. Every
  call over the same bytes returns the same trajectory; the recorded
  `ir_content_hash` is recomputed, never echoed from the document.
- **Same semantics as the approval identity.** Error variants
  (`EntryNotFound`, `JudgeBranch`, `Hash`) and their Display text, the
  `default`-branch judge rule, wait classification/order, effect rollup and
  node ordering are contract, not implementation detail: approve, Import and
  Apply compare against them.
- **Over-approximation.** Default branches only; a judge's non-default lanes and
  their descendants are invisible, and a predicted wait may never fire.

## Dependencies

IR plus the generated trajectory contract and `thiserror`. Never a capability,
engine, SQL or runtime edge.
