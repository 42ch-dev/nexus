# @42ch/nexus-ui

Nexus brand assets, design tokens, theme helpers, **React brand components** (`<NexusLogo>`, `<NexusMark>`), and **presentational UI primitives** (`<Button>`, `<Badge>`, `<Card>`, `<Input>`, `<Label>`, `<Textarea>`, `<Select>`). Ships as a workspace package consumed by `apps/web` and `apps/design-studio`.

## Install (workspace)

```bash
pnpm add @42ch/nexus-ui --workspace
```

## Public exports

| Entry | Description |
|-------|-------------|
| `@42ch/nexus-ui` | Brand token constants (`brandColors`, `logoVariants`, sizing guidance) + React components (`<NexusLogo>`, `<NexusMark>`, `<NexusLogoVariant>`, promoted UI primitives, `cn`) |
| `@42ch/nexus-ui/tokens` | Same token module (direct import) |
| `@42ch/nexus-ui/theme.css` | Brand CSS custom properties (`--nexus-brand-*`) |
| `@42ch/nexus-ui/assets/logos/logo-primary.svg` | Default lockup — bright mark on brand deep-blue plate (`logo-primary.png`) |
| `@42ch/nexus-ui/assets/logos/logo-white-bg.svg` | Lockup on white plate — only when a light surface is required (`logo-white-bg.png`) |
| `@42ch/nexus-ui/assets/logos/logo-white.svg` | Timeline mark — dark-gray→white gradient for dark heroes |
| `@42ch/nexus-ui/assets/logos/logo-mono.svg` | Timeline mark — light-gray→black gradient (static) |
| `@42ch/nexus-ui/assets/logos/logo-text.svg` | Wordmark — lowercase `nexus` (`currentColor`) |

### Promoted primitives

| Component | Import | Variants | Notes |
|-----------|--------|----------|-------|
| `NexusLogo` | `import { NexusLogo } from '@42ch/nexus-ui'` | `variant` (`primary`, `whiteBg`, `white`, `mono`, `text`) + consumer `src` | Bundler-agnostic `<img>`; plate lockups + wide marks + wordmark |
| `NexusMark` | `import { NexusMark } from '@42ch/nexus-ui'` | `size`, `label`, `className` | Inline timeline mark; `currentColor`; height-driven / `w-auto` |
| `NexusLogoVariant` | `import { NexusLogoVariant } from '@42ch/nexus-ui'` | `theme` (`elegant`, `nature`, `parchment`, `scifi`) + optional `palette` | Studio-only specimens; no assets; not a product theme switcher |
| `Button` | `import { Button } from '@42ch/nexus-ui'` | `variant` (`primary`, `secondary`, `tertiary`, `destructive`) + `size` (`small`, `default`, `large`) + `asChild` | Presentational only; no daemon or routing state |
| `Badge` | `import { Badge } from '@42ch/nexus-ui'` | `variant` (`neutral`, `running`, `queued`, `warning`, `error`, `preset`) + `tone` (`soft`, `solid`; default `soft`) | 24px status pill; soft = tinted fill + strengthened border; solid = semantic fill + high-contrast text (opt-in) |
| `Card` | `import { Card, CardHeader, CardTitle, CardDescription, CardContent } from '@42ch/nexus-ui'` | Five related sub-primitives; no variant axis | `Card` wraps content with border + shadow; `CardHeader`/`CardContent` layout helpers |
| `Input` | `import { Input } from '@42ch/nexus-ui'` | `invalid?: boolean` + native input attrs | V1.100 form-field contract; app owns id/describedby/copy |
| `Label` | `import { Label } from '@42ch/nexus-ui'` | Native label attrs (`htmlFor`) | Presentational `<label>`; app owns association IDs |
| `Textarea` | `import { Textarea } from '@42ch/nexus-ui'` | `invalid?: boolean` + native textarea attrs | Same invalid/`aria-invalid` pattern as Input |
| `Select` | `import { Select } from '@42ch/nexus-ui'` | `invalid?: boolean` + native select attrs | V1.101 native `<select>`; app owns `<option>` children; no Radix compound parts |

All primitives are named root exports — no deep subpath imports. Variant helpers (`buttonVariants`, `badgeVariants`) are internal implementation details; do not import them from the package.

### Transitional policy for unpromoted primitives

Components that have NOT been promoted to `@42ch/nexus-ui` remain in `apps/web/src/components/ui/` and can be imported through the project-local `@/components/ui` alias or the `@web-ui/*` barrel. Components classified `keep-web` (`Dialog`, `Tabs`, `Table`, `States`) stay app-owned until a future promotion plan locks their contract.

PNG provenance (`logo-primary.png`, `logo-white-bg.png`, `logo-mono.png`, `logo-text.png`, `logo-variants-*.png`) lives under `assets/logos/` (Git LFS). **Consumers should use SVG variants**, not PNGs, in product UI.

## Logo variant selection

Plate lockups (`primary`, `whiteBg`) are **square**. Transparent marks (`white`, `mono`) and `<NexusMark>` are **wide** (`viewBox` 284×28) — size by **height**; width is auto.

| Surface | Variant | File | Notes |
|---------|---------|------|-------|
| Product shell / default | Primary | `logo-primary.svg` | Bright mark on brand deep-blue plate (square) |
| Light/white plate only | White-bg | `logo-white-bg.svg` | Same mark on white plate — use only when deep-blue plate is wrong |
| Dark hero / photography / high-contrast panel | White | `logo-white.svg` | Dark-gray→white gradient on deep or busy backgrounds |
| Static grayscale lockup | Mono | `logo-mono.svg` | Light-gray→black gradient (baked) |
| Inline UI (buttons, badges, list rows) | Tintable | `<NexusMark>` | Set `color` on parent; inherits via `currentColor` |
| Wordmark lockup | Text | `logo-text.svg` | Lowercase `nexus`; `currentColor` (white on dark heroes) |
| Studio theme specimens only | — | `<NexusLogoVariant>` | Palette props; not a product theme switcher |

### Accessibility

- **Alt text**: use `alt="Nexus"` on `<img>`; inline SVGs include `<title>` for screen readers.
- **Minimum size**: 24px height (`logoMinSizePx` in tokens). Below this, node detail may be lost.
- **Clear space**: keep padding ≥ 25% of logo height on all sides.
- **Contrast**: prefer `logo-primary` by default. Use `logo-white-bg` only on light/white plates; `logo-white` on dark heroes.

## Runtime dependencies

The package carries its own runtime dependencies for class composition and primitive behavior:

| Dependency | Role | Why package dep (not peer) |
|------------|------|---------------------------|
| `class-variance-authority` | Variant API (`cva`, `VariantProps`) | Non-singleton — each consumer can ship its own instance |
| `@radix-ui/react-slot` | `asChild` pattern in `Button` | Non-singleton — no shared Radix context needed |
| `clsx` + `tailwind-merge` | Internal `cn` helper for class de-duplication | Non-singleton; package-local configuration with DESIGN.md token class-group extension |

`react` (>=18) and `react-dom` (>=18) remain peer dependencies — consumers supply their own React instance.

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

### Promoted primitives

```tsx
import { Button, Badge, Card, CardHeader, CardTitle, CardDescription, CardContent } from '@42ch/nexus-ui';

// Button with variant + size + asChild
<Button variant="primary" size="large">Create Work</Button>
<Button variant="tertiary" asChild>
  <a href="/settings">Settings</a>
</Button>

// Badge with semantic status variants (default tone=soft)
<Badge variant="running">Running</Badge>
<Badge variant="error">Failed</Badge>
// Opt-in solid / emphasis tone
<Badge tone="solid" variant="running">Running</Badge>

// Card with sub-primitives
<Card>
  <CardHeader>
    <CardTitle>Project Overview</CardTitle>
    <CardDescription>Last updated 2 hours ago</CardDescription>
  </CardHeader>
  <CardContent>
    <p>Your works are ready.</p>
  </CardContent>
</Card>
```

### Brand components

```tsx
import { NexusLogo, NexusMark, NexusLogoVariant } from '@42ch/nexus-ui';

// The consumer resolves the SVG URL through its own bundler.
// In a Vite project, importing an SVG yields a URL string:
import logoPrimary from '@42ch/nexus-ui/assets/logos/logo-primary.svg';
import logoText from '@42ch/nexus-ui/assets/logos/logo-text.svg';

function AppShell() {
  // Default product lockup — deep-blue plate. Pass the resolved URL as `src`.
  return <NexusLogo variant="primary" src={logoPrimary} size={24} />;
}

// Wordmark (currentColor — set color on parent for light/dark):
function Wordmark() {
  return (
    <span style={{ color: '#FFFFFF' }}>
      <NexusLogo variant="text" src={logoText} size={28} />
    </span>
  );
}

// Inline mono timeline mark — inherits color via `currentColor` (no SVG asset needed):
function Badge() {
  return (
    <span style={{ color: '#0D2B3E' }}>
      <NexusMark size={24} className="w-auto" />
    </span>
  );
}

// Studio Brand specimens only (palette props; not a product theme switcher):
function Specimen() {
  return <NexusLogoVariant theme="elegant" size={32} />;
}
```

**Why `<NexusLogo>` takes a `src` prop.** The package itself cannot bundle SVG assets — its build tool (`tsup` / `esbuild`) does not resolve `.svg` imports. Making SVG resolution the *consumer's* responsibility keeps the package bundler-agnostic: any consumer (Vite, webpack, Rollup, etc.) imports the SVG file through its own loader and passes the resulting URL. `<NexusMark>` / `<NexusLogoVariant>` sidestep this by inlining timeline geometry as hand-authored JSX. See `AGENTS.md` § *Component export strategy*.

### Raw SVG URL (without the React component)

If you want the logo asset without the component wrapper — for example, a plain `<img>` in a non-React surface — import the SVG directly:

```ts
import nexusLogo from '@42ch/nexus-ui/assets/logos/logo-primary.svg';

// <img src={nexusLogo} alt="Nexus" height={24} style={{ width: 'auto' }} />
```

## Cross-surface compatibility

- Export paths are stable public API — future `nexus-platform` and other surfaces should import only documented entries.
- Do not deep-import `src/` or undocumented paths.
- Full brand token SSOT: root `DESIGN.md` / `DESIGN.dark.md` — all app surfaces consume these directly.

## Development

```bash
pnpm --filter @42ch/nexus-ui run build
pnpm --filter @42ch/nexus-ui run typecheck
```

## Roadmap

### Current API (0.2.0)

- **React brand components**: `<NexusLogo variant="..." src="...">` (presentational, explicit variant, `<img>`-based) and `<NexusMark>` (inline mono SVG, `currentColor`). React 18+ peer deps.
- **UI primitives**: `<Button>`, `<Badge>`, `<Card>`, `<Input>`, `<Label>`, `<Textarea>`, `<Select>` — pure presentational, token-driven, compatible with both `apps/web` and `apps/design-studio`. Variant helpers stay internal; no deep subpath exports.
- **Class composition**: package-local `cn` helper with DESIGN.md token class-group extension via `tailwind-merge` (public `cn` export).

### Deferred

- Layout primitives (Header/Sidebar/RootLayout) — coupled to app routing/state.
- npm publish (workspace-only for now).
- ThemeProvider consolidation into this package.
- Field groups / FormField composition — out of package scope (app-owned).
- Radix compound Select (Trigger/Value/Item) — separate future plan if needed.
