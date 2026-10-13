# evidence before retirement — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/workflow-patterns/evidence-before-retirement.md`](../../../knowledge/workflow-patterns/evidence-before-retirement.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 16

```markdown
  - apps/desktop (retired)
```

### at pre-freeze line 35

```markdown
v1.192 retired the replaced native desktop composition (`apps/desktop/**`, `scripts/fetch-sidecar.sh`, its release workflow, its plugin package family and the `desktop` workspace package) after the Electron host was accepted — one desktop host must remain. In the same iteration, two neighboring families were **not** deleted:

- the legacy integrated daemon + Daemon API + embedded SPA, and
- dormant/hidden CLI route rows,

both of which *look* obsolete but still have live consumers. The retirement round's most reusable output is the discipline that separates the two outcomes: a source-reverified consumer inventory, an adjudicated-keep record, and a closeout ledger that makes documentation lag a tracked quantity instead of a discovered surprise.
```

### at pre-freeze line 51

```markdown
Concrete v1.192 findings worth imitating: default features were still `legacy-cli` + `web-embed`; `daemon start` still booted the integrated daemon; the release still embedded and served the SPA; `BrowserClient` was contract-bound to `/v1/daemon/*`. The open residual `R-V1190-P6T4-REMAINING-LEAVES` remained the named blocker. Note the trap: the residual's detail text partly described a *working tree ahead of `main`* — inventory against `main`, not against the plan branch that proposed the deletion.
```

### at pre-freeze line 55

```markdown
Every "dormant" CLI row existed in code with a clap definition and a `run` path — hard-reject stubs (`creator workspace clone`), coming-soon prints (`workspace link|unlink|status`, `publish`), config-error/exit-2 stubs (`explore browse|search`, `platform context assemble`), and hidden-but-callable aliases (`sync`, `preset`, `capability`). One row was even *visible* while the spec said hidden. Deletion of such rows requires a per-row disposition plus help/docs/parity proof — "no callers in tests" is not obsolescence.
```

### at pre-freeze line 87

```markdown
Before declaring the closeout done, grep for executable callers of every deleted script/directory across owned scopes **and** outside them. v1.192's out-of-scope find: a closed measurement harness still spawned the deleted `scripts/fetch-sidecar.sh` for a surface that was already non-functional at baseline. The disposition (remove the broken surface rather than restore the deleted script) belongs to PM; the *finding* belongs to the closeout. Stale comment-only citations and historical records are fine; live executable references are not.
```

### at pre-freeze line 95

```markdown
Closing dependabot PRs (#310/#312) or any external cleanup executes only **after the retirement verifies merged to `main`** — never because a feature worktree deleted the files. The preconditions are explicit: confirm the PRs still target the retired tree, comment with the reason and the merge SHA, close, record final states/URLs as post-merge receipts. Related post-merge obligations (branch-protection required check names, CI green runs) are recorded at closeout even when they execute later.
```

### at pre-freeze line 103

```markdown
v1.212 (p3 T3) deleted two serde fields from `nexus-orchestration` capability
structs as "unread":
```

### at pre-freeze line 139

```markdown
- **The ledger is the memory.** Six months later, the only way to know why the daemon/SPA composition still exists is the adjudicated-keep record with its blocker and trigger.
- **Documentation lag outlives the code.** Sweeping current docs in the same wave (and listing what remains) is what makes "exactly one desktop host" true for readers, not just for the build.
```

### at pre-freeze line 144

```markdown
- Any host/composition/dependency retirement, including the future daemon/SPA and dormant-CLI families whose blockers are recorded.
```

### at pre-freeze line 165

```markdown
| Legacy daemon/SPA composition deletion | `R-V1190-P6T4-REMAINING-LEAVES` (open, high); trigger: default-feature switch completed on main + consumers at zero + new inventory | roadmap RFT-11 slice; project register |
```

### at pre-freeze line 171

```markdown
1. gh pr view 310 / 312 — confirm still open and targeting the retired tree.
```

### at pre-freeze line 173

```markdown
3. gh pr close 310 / 312; record final states and URLs.
```

### at pre-freeze line 176

```markdown

## Evidence

- Product-side retirement — deleted `apps/desktop/**` (21 tracked paths), `scripts/fetch-sidecar.sh`, `scripts/dev-backend-manifest.mjs` sidecar helpers; `.github/workflows/desktop-release.yml` removed and the native-host `desktop-build` job stripped in `.github/workflows/desktop-build.yml` / `ci.yml`; lockfile 17 → 16 workspace projects with `sharp` preserved.
- Retained/adjudicated families — `apps/nexus42` default features, daemon boot path, embedded SPA serving, and the dormant CLI rows listed in `.mstar/specs/active/architecture/rust-core-service-boundary.md` §7.4.
- Policy texts — `.github/dependabot.yml` (all remaining directories verified at HEAD), `.oxlintrc.json` (dead override removed), `.mstar/specs/active/surfaces/desktop-shell.md` header, `.mstar/specs/README.md` rows.
- Companion docs — [resolved-residual-verification.md](../../../knowledge/architecture-patterns/resolved-residual-verification.md) (verify residual claims against current `main`), [surface-rename-hygiene-checklist.md](../../../knowledge/conventions/surface-rename-hygiene-checklist.md) (sweep shape for cross-language renames), [capability-parity-receipt.md](../../../knowledge/workflow-patterns/capability-parity-receipt.md) (the acceptance-evidence counterpart for this cutover).
```
