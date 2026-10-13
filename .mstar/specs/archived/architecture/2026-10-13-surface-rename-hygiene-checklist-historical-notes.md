# surface rename hygiene checklist — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/conventions/surface-rename-hygiene-checklist.md`](../../../knowledge/conventions/surface-rename-hygiene-checklist.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 61

```markdown
**These greps are a pre-commit self-check for the sweep executor, not an "available" reference** — run them and resolve every hit before committing the sweep. V1.93 skipped this and the stutters reached QC (see V1.93 lesson below).
```

### at pre-freeze line 89

```markdown
## V1.90 lesson

The first implementation pass left `/v1/local/` and "Local API" references in 11+ files across `crates/nexus-agent-host`, `crates/nexus-orchestration`, `crates/nexus-local-db`, `apps/AGENTS.md`, `apps/desktop-electron/src/main.ts`, and an embedded preset prompt. A dedicated grep sweep in P-last caught them before merge. The fix was text-only but required three QC rounds; running the sweep as part of the initial rename would have saved the extra review cycles.

## V1.93 lesson (naming-sweep stragglers — R-V192HYG-001)

V1.93 ran a "Local API" → "Daemon API" straggler sweep (135 line changes across 27 live-prose files). Two failure classes reached QC because **the sweep executor did not run the §3 verification greps before committing**:

- **W-001 — stutter/double-naming**: the mechanical replace over-reached where the surrounding text already referenced "Daemon API" or where "Local" was part of an already-renamed phrase, producing "Daemon Daemon API", "renaming from Daemon API to Daemon API" (should be "from **Local API** to Daemon API" — the *from* side names the historical api), "## 6. Daemon Daemon API (principles)". The §3 stutter grep (`daemon Daemon API` etc.) existed in this checklist but was not run; QC1 caught 6 files + 11 artifacts.
- **W-002 — broken anchor**: a TOC entry's text was renamed ("Local API Contract Analysis" → "Daemon API Contract Analysis") but the anchor slug `#4-local-api-contract-analysis` was not updated to `#4-daemon-api-contract-analysis`. The §3 anchor-link verification gate above did not yet exist in this checklist; it was added as a result of V1.93.

Takeaways: (1) the §3 greps + anchor check are a **mandatory pre-commit self-check** for any rename sweep, not optional; (2) where a construction is "renaming from X to Y", the *X* side is the historical name and must stay — the replace must be context-aware, not a blind token swap; (3) a fix-wave correcting the stutters + anchors landed in the same iteration (commit `ef1b4efa`) before merge.

```

### at pre-freeze line 113

```markdown
## V1.214 lesson (retired-surface sweep — native desktop host)

The native desktop host was retired at v1.192, but its tokens survived in 85 tracked files: a dedicated agent skill for that host, dead comment/path references to its removed source tree, a host-prefixed drag-region attribute with **no consumer**, and static literals whose only purpose was to forbid the retired host. Two dispositions generalise: (a) the retired host's package-scope import ban and the service-origin negative assertions were **dead rules** — their object was gone and the same files carried stronger assertions (an exact-set `deepEqual`, other `forbid` rules), so they were deleted rather than preserved; (b) the one serialized receipt key that literally contained the token was renamed under explicit authorization with a recorded consumer disposition, because that is a shape change rather than a deletion.
```
