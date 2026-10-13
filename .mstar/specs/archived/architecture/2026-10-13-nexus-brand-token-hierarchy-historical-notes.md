# nexus brand token hierarchy — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/architecture-patterns/nexus-brand-token-hierarchy.md`](../../../knowledge/architecture-patterns/nexus-brand-token-hierarchy.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 26

```markdown
**Refreshed 2026-09-10** — re-based on v1.187 v0.5 "Precision Creative Tool" (cobalt / silver-neutral / graphite, offline system typography, compiler-owned projection); the v0.4 warm-paper/serif/cyan values it previously described are retired.
```

### at pre-freeze line 28

```markdown
**Track**: Knowledge (durable guidance distilled from V1.83 Brand UI Foundation; corrected V1.94; unified V1.98; recalibrated V1.121 v0.4 Literary Engine; re-based on v1.187 v0.5 "Precision Creative Tool").
```

### at pre-freeze line 32

```markdown
Before V1.83, `apps/web/DESIGN.md` held both app-specific canvas/SOUL/findings tokens and de-facto brand colors. V1.83 introduced a publishable `@42ch/nexus-ui` package and root `DESIGN.md` / `DESIGN.dark.md` as the cross-application brand SSOT. **V1.98 unified the full token contract**: the root pair is the **sole** normative token SSOT (not just brand), `apps/web/DESIGN*.md` were deleted, and a shared `@nexus/design-tokens` workspace package (Tailwind preset + `tokens.css`) was extracted at `tooling/design-tokens/` so every app consumes one pipeline with no per-app duplicate `theme.extend`. A read-only `apps/design-studio` gallery visualizes the contract.
```

### at pre-freeze line 34

```markdown
**v1.187 (`version: 0.5.0`)** recalibrated the language to a precision creative tool: silver-neutral light planes, graphite dark planes, cobalt interaction, offline system typography. The v0.4 warm-paper surfaces, global literary serif and cyan glow are retired values — they are not an alternative theme, and the public token *names* (`brand-deep-blue`, `brand-cyan`, `blue-*`, `brandColors.deepBlue/cyan/white`) are stable API identifiers that now carry the new pigments. Exact values live in the DESIGN pair; do not restate them elsewhere.
```

### at pre-freeze line 70

```markdown
### Registered-scale-only rule (V1.98 lesson — qc1 W001)
```

### at pre-freeze line 74

```markdown
### Audit pattern (V1.94, still the shape to copy)
```

### at pre-freeze line 98

```markdown
- Resurrect a per-app `DESIGN.md` mapping layer (V1.98 retired `apps/web/DESIGN*.md`); the root pair + `@nexus/design-tokens` is the SSOT.
```

### at pre-freeze line 101

```markdown
- Use a Tailwind scale step that is not registered in `tooling/design-tokens/tailwind.preset.ts` (it silently emits nothing in production — V1.98 qc1 W001).
```

### at pre-freeze line 105

```markdown
- Treat the public name `brand-cyan` as a colour promise — it carries the theme's cobalt signal now, and the frozen logo assets intentionally keep their historical pigments.
- Reintroduce the retired v0.4 language (warm-paper surfaces, serif display tier, neon cyan glow) as an alternative palette or a "light reset" scope.
```

### at pre-freeze line 119

```markdown
## Chronos dual-role + timeline logo (`2026-07-22-vi-logo-upgrade`, values re-based v1.187)
```

### at pre-freeze line 132

```markdown
### Failure mode that cost a QC fix-wave (W-001) — the durable lesson
```

### at pre-freeze line 134

```markdown
After a scale reinterpretation, surfaces that still meant **deep structure** but used the generic interaction scale (`border-blue-700/20 bg-blue-700/10 text-blue-700`) rendered as accent washes — fingerprint trust/match cards on connection-setup violated `DESIGN.md` `connection-setup.security-note`. Fix: deep-ink classes (`border-brand-deep-blue/20`, `bg-brand-deep-blue/10`, `text-brand-deep-blue`) plus regression assertions that **exclude** the generic scale on those nodes.
```

### at pre-freeze line 138

```markdown
### Timeline logo system (V1.132 plain vs square split — assets unchanged by v0.5)
```

### at pre-freeze line 152

```markdown
**Asset boundary (v0.5):** the SVG/PNG assets keep their historical baked pigments. That is deliberate: installed-identity assets are references, not live token swatches. `NexusMark` is the inline `currentColor` mark driven by shared tokens; Brand separates installed asset references from live token/CSS swatches. Do not recolor assets with CSS filters to simulate a new logo, and do not regenerate desktop icons from this change.
```

### at pre-freeze line 154

```markdown
**macOS app icon (V1.135 P1 / V1.136 — desktop-domain, unaffected by v0.5):**
```

### at pre-freeze line 156

```markdown
1. **H1 (retain):** Canvas must stay **fully opaque RGB** (`hasAlpha: false`) — never transparent margins/`INSET_RATIO` alpha that defeat the macOS squircle mask.
```

### at pre-freeze line 158

```markdown
3. **H6 contrast (V1.136):** Margin/plate **must not share the same hex** — same-colour bakes yield 0 non-plate border pixels (geometry exists but is invisible). Use a contrasting margin and verify with a border-pixel scan before claiming compose success.
```

### at pre-freeze line 162

```markdown
**Removed:** `logo-color.svg` / `logoVariants.color` — redundant; do not resurrect.
```

### at pre-freeze line 170

```markdown
- Button: assert the light primary recipe (`blue-700` fill + `brand-white` label) and the dark primary recipe (`blue-700` fill + `brand-deep-blue` label); assert absence of the retired cyan-fill primary.
```

### at pre-freeze line 178

```markdown
## v0.5 design-language additions (v1.187)
```

### at pre-freeze line 180

```markdown
These replaced the v0.4 values wholesale; the DESIGN pair is the value authority, the rules below are the durable part.
```

### at pre-freeze line 194

```markdown
- The V1.121 Source Serif 4 font-face declarations and Studio preloads are **removed from the active pipeline**; the pattern that wired them (and what a future reintroduction must do differently, given `tokens.css` is now generated) is recorded in [self-hosted-ofl-font-wiring.md](../../../knowledge/architecture-patterns/self-hosted-ofl-font-wiring.md).
```

### at pre-freeze line 207

```markdown
### twMerge registry hardening (V1.121, retained)
```

### at pre-freeze line 222

```markdown
**Threat model**: the V1.94 silent-strip class of bug — an unregistered display-size class misparsed as a text-colour class and dropped by `twMerge`. `packages/nexus-ui/src/lib/cn.test.ts` asserts representative classes from each new group survive `twMerge()` against conflicting defaults.
```

### at pre-freeze line 226

```markdown
`pnpm --filter @nexus/design-tokens check` regenerates the three derived artifacts from the DESIGN pair and compares them byte-for-byte, plus asserts leaf/var parity and rejects stale v0.4 pins. It **replaced** the older hand-written needle list: the compiler's fail-closed rules and the byte comparison are what make a landing provable now — see [design-pair-token-compiler.md](../../../knowledge/architecture-patterns/design-pair-token-compiler.md) and [component-variant-token-projection.md](../../../knowledge/architecture-patterns/component-variant-token-projection.md).
```

### at pre-freeze line 230

```markdown
Every canvas/outline/worldkb/timeline/SOUL/annotation colour resolves from the brand semantic scales (no Tailwind-palette leftovers). Per-surface accent spines stay distinct (strategy / outline / worldkb / timeline / layer accents) and canvas stays neutral with a subtle decorative grid; hierarchy comes from the node plane, border, selection ring/state text and semantic spine. The v0.4 hue-mapping appendix is retired — the rule is now simply "these families project from the same scales as everything else".
```
