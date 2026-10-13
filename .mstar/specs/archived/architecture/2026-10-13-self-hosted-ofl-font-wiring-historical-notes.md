# self hosted ofl font wiring — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/architecture-patterns/self-hosted-ofl-font-wiring.md`](../../../knowledge/architecture-patterns/self-hosted-ofl-font-wiring.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 24

```markdown
**Refreshed 2026-09-10** — v1.187 v0.5: no self-hosted font is active; the V1.121 Source Serif 4 instance is retired from the pipeline, and the generated `tokens.css` must never be hand-edited to re-add one.
```

### at pre-freeze line 26

```markdown
**Track**: Knowledge (durable procedure distilled from the V1.121 v0.4 display-tier implementation; amended for the v1.187 generated-token pipeline).
```

### at pre-freeze line 28

```markdown
## Status in the current language (v0.5)
```

### at pre-freeze line 30

```markdown
**No self-hosted font is active.** The v1.187 "Precision Creative Tool" language resolves interface *and* content display to the offline OS sans stack (`system-ui` + Apple/Segoe UI + CJK system fallbacks). Consequences to keep straight:
```

### at pre-freeze line 33

```markdown
- Unused packaged font files and app `public/fonts/**` copies may still exist on disk from V1.121 — they are inert remains, not an active typography authority. Do not treat their presence as evidence that a font is wired.
```

### at pre-freeze line 36

```markdown
**The pipeline change that matters most if you do reintroduce one:** `tokens.css` is now a **generated artifact**. The V1.121 procedure below placed `@font-face` blocks in that file by hand; doing that today would be overwritten by the next `pnpm --filter @nexus/design-tokens generate` (and would fail the byte-compare gate). A future font must be wired either by teching the compiler to emit it from a DESIGN source field, or by adding a separate handwritten shared CSS module imported alongside the generated one by each app. Never hand-edit generated output.
```

### at pre-freeze line 38

```markdown
## Context (the original problem, still the pattern's rationale)
```

### at pre-freeze line 40

```markdown
V1.121 needed a literary serif (Source Serif 4) as the content-voice typeface. Nexus is a local-first product — no webfont CDN dependency. The font also had to respect the `@42ch/nexus-ui` bundler-agnostic asset boundary (see [bundler-agnostic-component-library-assets.md](../../../knowledge/architecture-patterns/bundler-agnostic-component-library-assets.md)): a tsup-built React component library cannot import `.woff2` binaries in source.
```

### at pre-freeze line 66

```markdown
- **v0.5 amendment:** declarations cannot live in the generated `tokens.css`. Emit them through the compiler (preferred, keeps one pipeline) or place them in a handwritten shared CSS import that every app loads next to the generated file.
```

### at pre-freeze line 76

```markdown
- Gate the gzipped `.woff2` size per weight (V1.121 used ≤ 80 KB gz/weight and measured 19.7 KB gz (400) / 21.1 KB gz (600) for a Latin subset).
```

### at pre-freeze line 98

```markdown
## Examples

### V1.121 (historical — retired by v1.187)

| Layer | Path | Note |
|-------|------|------|
| Canonical + OFL | `packages/nexus-ui/assets/fonts/source-serif-4/` | LFS-tracked binary + `OFL.txt` |
| App vendored subset | `apps/web/public/fonts/source-serif-4/`, `apps/design-studio/public/fonts/source-serif-4/` | `*.woff2` + `PROVENANCE.md` (files may remain on disk, unused) |
| `@font-face` + `--font-display` | `tooling/design-tokens/src/tokens.css` | Removed from the generated pipeline; must not be re-added by hand |
| Preload | app `index.html` | Removed from the active pipeline |
| Bundle gate | V1.121 plan AC-P0-5 | 19.7 / 21.1 KiB gz — PASS (≤ 80 KB) |

```
