# @42ch/nexus-ui

Nexus brand assets, design tokens, and theme helpers. Framework-neutral — **no React components** in V1.83.

## Install (workspace)

```bash
pnpm add @42ch/nexus-ui --workspace
```

## Public exports

| Entry | Description |
|-------|-------------|
| `@42ch/nexus-ui` | Brand token constants (`brandColors`, `logoVariants`, sizing guidance) |
| `@42ch/nexus-ui/tokens` | Same token module (direct import) |
| `@42ch/nexus-ui/theme.css` | Brand CSS custom properties (`--nexus-brand-*`) |
| `@42ch/nexus-ui/assets/logos/logo-primary.svg` | Deep blue mark (`#1E3A5F`, flat primary) for light backgrounds |
| `@42ch/nexus-ui/assets/logos/logo-color.svg` | Cyan mark (`#25D1E0`) — bright logo for dark backgrounds |
| `@42ch/nexus-ui/assets/logos/logo-white.svg` | White mark (`#FFFFFF`) |
| `@42ch/nexus-ui/assets/logos/logo-mono.svg` | Monotone mark (`currentColor`) |

PNG source references (`logo_dark.png`, `logo_light.png`, `logo_white.png`) live under `assets/logos/` for provenance and are tracked via Git LFS. **Consumers should use SVG variants**, not PNGs, in product UI.

## Logo variant selection

| Surface | Variant | File | Notes |
|---------|---------|------|-------|
| Light nav / sidebar (light theme) | Deep blue | `logo-primary.svg` | Default shell mark on white/light gray backgrounds |
| Dark nav / sidebar (dark theme) | Cyan | `logo-color.svg` | Bright mark on dark chrome |
| Dark hero / photography / high-contrast panel | White | `logo-white.svg` | Maximum contrast on deep or busy backgrounds |
| Inline UI (buttons, badges, list rows) | Monotone | `logo-mono.svg` | Set `color` on parent; inherits via `currentColor` |
| Favicon / small chrome (optional) | Deep blue or mono | `logo-primary.svg` or `logo-mono.svg` | Prefer mono when tinting |

### Accessibility

- **Alt text**: use `alt="Nexus"` on `<img>`; inline SVGs include `<title>` and `<desc>` for screen readers.
- **Minimum size**: 24px height (`logoMinSizePx` in tokens). Below this, node detail may be lost.
- **Clear space**: keep padding ≥ 25% of logo height on all sides.
- **Contrast**: pick white or cyan on dark backgrounds; deep blue or white on light backgrounds. Do not place cyan on light gray without a contrast check.

## Usage examples

### TypeScript tokens

```ts
import { brandColors, logoVariants } from '@42ch/nexus-ui';

const navLogo = logoVariants.primary;
const accent = brandColors.cyan;
```

### CSS theme

```css
@import '@42ch/nexus-ui/theme.css';

.shell-header {
  color: var(--nexus-brand-deep-blue);
}
```

### Vite / bundler (SVG URL)

```ts
import nexusLogo from '@42ch/nexus-ui/assets/logos/logo-primary.svg';

// <img src={nexusLogo} alt="Nexus" width={32} height={32} />
```

### Inline mono SVG

```tsx
import { ReactComponent as NexusMark } from '@42ch/nexus-ui/assets/logos/logo-mono.svg';
// Or import raw SVG and render with className="text-foreground"
```

> V1.83 does not ship React components. P2 `apps/web` wires logos directly.

## Cross-surface compatibility

- Export paths are stable public API — future `nexus-platform` and other surfaces should import only documented entries.
- Do not deep-import `src/` or undocumented paths.
- Full brand token SSOT: root `DESIGN.md` / `DESIGN.dark.md` (P1). Web-specific mappings: `apps/web/DESIGN*.md` (P1/P2).

## Development

```bash
pnpm --filter @42ch/nexus-ui run build
pnpm --filter @42ch/nexus-ui run typecheck
```

## Roadmap (out of scope V1.83)

- React component library (`<NexusLogo />`, layout primitives) — deferred to a later plan after P2 proves stable usage.
- npm publish — not in this iteration.
