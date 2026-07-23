# @42ch/nexus-ui

Nexus brand assets, design tokens, theme helpers, **React brand components** (`<NexusLogo>`, `<NexusMark>`), and **presentational UI primitives** (`<Button>`, `<Badge>`, `<Card>`, `<Input>`, `<Label>`, `<Textarea>`, `<Select>`, `<Tabs>`). Ships as a workspace package consumed by `apps/web` and `apps/design-studio`.

## Install (workspace)

```bash
pnpm add @42ch/nexus-ui --workspace
```

## Public exports

| Entry | Description |
|-------|-------------|
| `@42ch/nexus-ui` | Brand token constants (`brandColors`, `logoVariants`, `logoSquareVariants`, `logoCompactMarkHeightPx`, sizing guidance) + React components (`<NexusLogo>`, `<NexusMark>`, `<NexusLogoVariant>`, promoted UI primitives, `cn`) |
| `@42ch/nexus-ui/tokens` | Same token module (direct import) |
| `@42ch/nexus-ui/theme.css` | Brand CSS custom properties (`--nexus-brand-*`) |
| `@42ch/nexus-ui/assets/logos/logo-primary.svg` | Plain wide primary timeline mark (no plate) |
| `@42ch/nexus-ui/assets/logos/logo-white-bg.svg` | Plain wide mark for light surfaces (no plate) |
| `@42ch/nexus-ui/assets/logos/logo-primary-square.svg` | Square deep-blue plate lockup (sidebar, compose source) |
| `@42ch/nexus-ui/assets/logos/logo-white-bg-square.svg` | Square white plate lockup |
| `@42ch/nexus-ui/assets/logos/logo-white.svg` | Timeline mark — dark-gray→white gradient for dark heroes / ink titlebar |
| `@42ch/nexus-ui/assets/logos/logo-mono.svg` | Timeline mark — light-gray→black gradient (static) |
| `@42ch/nexus-ui/assets/logos/logo-text.svg` | Wordmark — lowercase `nexus` (`currentColor`) |

### Promoted primitives

| Component | Import | Variants | Notes |
|-----------|--------|----------|-------|
| `NexusLogo` | `import { NexusLogo } from '@42ch/nexus-ui'` | `variant`, `src`, `size?`, `label?`, `className?`, `draggable?` | Bundler-agnostic `<img>`; plate lockups + wide marks + wordmark; set `draggable={false}` in titlebar chrome to avoid native image ghost-drag |
| `NexusMark` | `import { NexusMark } from '@42ch/nexus-ui'` | `size`, `label`, `className` | Inline timeline mark; `currentColor`; height-driven / `w-auto` |
| `NexusLogoVariant` | `import { NexusLogoVariant } from '@42ch/nexus-ui'` | `theme` (`elegant`, `nature`, `parchment`, `scifi`) + optional `palette` | Studio-only specimens; no assets; not a product theme switcher |
| `Button` | `import { Button } from '@42ch/nexus-ui'` | `variant` (`primary`, `secondary`, `tertiary`, `destructive`) + `size` (`small`, `default`, `large`) + `asChild` | Presentational only; **primary is theme-split** — light shell: deep ink fill + white label; dark shell: cyan fill + deep label (VI-002) |
| `Badge` | `import { Badge } from '@42ch/nexus-ui'` | `variant` (`neutral`, `running`, `queued`, `warning`, `error`, `preset`) + `tone` (`soft`, `solid`; default `soft`) | 24px status pill; soft = tinted fill + strengthened border; solid = semantic fill + high-contrast text (opt-in) |
| `Card` | `import { Card, CardHeader, CardTitle, CardDescription, CardContent } from '@42ch/nexus-ui'` | Five related sub-primitives; no variant axis | `Card` wraps content with border + shadow; `CardHeader`/`CardContent` layout helpers |
| `Input` | `import { Input } from '@42ch/nexus-ui'` | `invalid?: boolean` + native input attrs | V1.100 form-field contract; app owns id/describedby/copy |
| `Label` | `import { Label } from '@42ch/nexus-ui'` | Native label attrs (`htmlFor`) | Presentational `<label>`; app owns association IDs |
| `Textarea` | `import { Textarea } from '@42ch/nexus-ui'` | `invalid?: boolean` + native textarea attrs | Same invalid/`aria-invalid` pattern as Input |
| `Select` | `import { Select } from '@42ch/nexus-ui'` | `invalid?: boolean` + native select attrs | V1.101 native `<select>`; app owns `<option>` children; no Radix compound parts |
| `Tabs` | `import { Tabs, TabsList, TabsTrigger, TabsContent } from '@42ch/nexus-ui'` | Controlled (`value` + `onValueChange`) or uncontrolled (`defaultValue`) compound | V1.137 React context tabs; a11y roles on list/trigger/panel |

All primitives are named root exports — no deep subpath imports. Variant helpers (`buttonVariants`, `badgeVariants`) are internal implementation details; do not import them from the package.

### Transitional policy for unpromoted primitives

Components that have NOT been promoted to `@42ch/nexus-ui` remain in `apps/web/src/components/ui/` and can be imported through the project-local `@/components/ui` alias or the `@web-ui/*` barrel. Components classified `keep-web` (`Dialog`, `Table`, `States`) stay app-owned until a future promotion plan locks their contract.

PNG provenance (`logo-primary.png`, `logo-white-bg.png`, `logo-mono.png`, `logo-text.png`, `logo-variants-*.png`) lives under `assets/logos/` (Git LFS). **Consumers should use SVG variants**, not PNGs, in product UI.

## Logo variant selection

**Plain vs square:** `logoVariants` maps to **wide plain marks** (no plate). `logoSquareVariants` maps to **square plate lockups** (`*-square.svg`). Do not substitute plain marks for plate lockups or vice versa.

Plate lockups (`logoSquareVariants`) are **square**. Plain marks (`logoVariants`) and `<NexusMark>` are **wide** (`viewBox` 284×28) — size by **height**; width is auto.

**Compact mark scale:** `logoCompactMarkHeightPx` (14px) is the shared SSOT for titlebar, Brand hero mini marks, and app-icon inner scale (−30% from `logoShellHeightPx` 20px). Sidebar plate lockups use `logoShellHeightPx`.

| Surface | Variant | File | Notes |
|---------|---------|------|-------|
| Sidebar / header plate | Square primary | `logo-primary-square.svg` | Deep-blue plate at `logoShellHeightPx` (20px) |
| Ink titlebar compact mark | Plain white | `logo-white.svg` | At `logoCompactMarkHeightPx` (14px) — not the plate lockup |
| Light/white plate only | Square white-bg | `logo-white-bg-square.svg` | Square plate when deep-blue is wrong |
| Plain timeline mark | Plain primary | `logo-primary.svg` | Wide mark without plate |
| Dark hero / photography | Plain white | `logo-white.svg` | Dark-gray→white gradient |
| Static grayscale lockup | Plain mono | `logo-mono.svg` | Baked gradient |
| Inline UI (buttons, badges, list rows) | Tintable | `<NexusMark>` | Set `color` on parent; inherits via `currentColor` |
| Wordmark lockup | Text | `logo-text.svg` | Lowercase `nexus`; `currentColor` |
| Studio theme specimens only | — | `<NexusLogoVariant>` | Palette props; not a product theme switcher |
| Desktop app icon compose | Square primary | `logo-primary-square.svg` | Opaque squircle plate on 1024×1024 canvas (6% inset, 22% radius) — see `apps/desktop/src-tauri/icons/README.md`; no transparent inset |

### Accessibility

- **Alt text**: use `alt="Nexus"` on `<img>`; inline SVGs include `<title>` for screen readers.
- **Minimum size**: 24px height (`logoMinSizePx` in tokens). Below this, node detail may be lost.
- **Clear space**: keep padding ≥ 25% of logo height on all sides.
- **Contrast**: square `logo-primary-square` for plate lockups; plain marks for wide timeline usage. Use `logo-white-bg-square` only on light/white plates; `logo-white` on dark heroes and ink titlebar.

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
import { brandColors, logoVariants, logoSquareVariants, logoCompactMarkHeightPx } from '@42ch/nexus-ui';

const plateLockup = logoSquareVariants.primary;
const titlebarMarkHeight = logoCompactMarkHeightPx;
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

// Button with theme-split primary (light: ink fill; dark: cyan fill)
<Button variant="primary" size="large">Create</Button>
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
import logoPrimarySquare from '@42ch/nexus-ui/assets/logos/logo-primary-square.svg';
import logoText from '@42ch/nexus-ui/assets/logos/logo-text.svg';

function AppShell() {
  // Sidebar plate lockup — square asset at logoShellHeightPx.
  return <NexusLogo variant="primary" src={logoPrimarySquare} size={20} />;
}

// Titlebar chrome: compact plain mark on ink (apps/web: NexusInkLogo wrapper).
import logoWhite from '@42ch/nexus-ui/assets/logos/logo-white.svg';
import { logoCompactMarkHeightPx } from '@42ch/nexus-ui';

function TitlebarLogo() {
  return (
    <NexusLogo
      variant="white"
      src={logoWhite}
      size={logoCompactMarkHeightPx}
      draggable={false}
    />
  );
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
import nexusPlate from '@42ch/nexus-ui/assets/logos/logo-primary-square.svg';

// <img src={nexusPlate} alt="Nexus" height={20} style={{ width: 'auto' }} />
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
- **UI primitives**: `<Button>`, `<Badge>`, `<Card>`, `<Input>`, `<Label>`, `<Textarea>`, `<Select>`, `<Tabs>` — pure presentational, token-driven, compatible with both `apps/web` and `apps/design-studio`. Variant helpers stay internal; no deep subpath exports.
- **Class composition**: package-local `cn` helper with DESIGN.md token class-group extension via `tailwind-merge` (public `cn` export).

### Deferred

- Layout primitives (Header/Sidebar/RootLayout) — coupled to app routing/state.
- npm publish (workspace-only for now).
- ThemeProvider consolidation into this package.
- Field groups / FormField composition — out of package scope (app-owned).
- Radix compound Select (Trigger/Value/Item) — separate future plan if needed.
