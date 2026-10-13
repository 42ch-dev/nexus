# AGENTS — Historical Notes

**Provenance**: frozen from [`AGENTS.md`](../../../../AGENTS.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed v1.190/v1.193/V1.122–V1.123 provenance wording; freeze date used).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 39

```markdown
- Headless runtime: **`nexus-runtime`** (Connect-only binary of the same crate as the CLI). The integrated `nexus42 daemon` group and the `nexus-daemon-runtime` composition were retired in v1.193 P2 — no `nexus42d`, no `nexus42 service` alias, no CLI service launcher
```

### at pre-freeze line 89

```markdown
UI work in this repo follows a **studio-first** routing rule. The visual proving ground is `apps/design-studio` (daemon-free Vite gallery); reusable presentational primitives live in `packages/nexus-ui`. Agents must **not** land a new visual system directly in `apps/web` and call it done — that is how V1.122/V1.123 Timeline visuals ended up with tokens in `@nexus/design-tokens` and implementations in `apps/web`, but no Studio gallery and no `@42ch/nexus-ui` representation. Visuals are tuned in Studio first; componentize so promotion stays cheap.
```

### at pre-freeze line 176

```markdown
**Merge gate — feature-branch target cleanup (HARD):** before merging a feature branch into the integration branch, that feature's scoped `CARGO_TARGET_DIR` **must be removed** — a feature target never outlives its own merge. With the scoped `.envrc` layout the removal is precise, by exact name, and never a wildcard: `rm -rf ~/.cache/nexus-target-*` would delete a peer's target while that peer is still building. This is not optional housekeeping — parallel worktree target dirs compound silently (v1.190: six concurrent targets consumed 98 GiB of `/tmp` in a single iteration) and degraded the host. The recorded conclusion is **immediate per-slice reclamation, not a cap on concurrent development**. Concretely:
```
