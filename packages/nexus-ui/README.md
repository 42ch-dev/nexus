# @42ch/nexus-ui

Nexus brand assets, design tokens, theme helpers, **React brand components** (`<NexusLogo>`, `<NexusMark>`), and **presentational UI primitives** (`<Button>`, `<Badge>`, `<Card>`, `<Input>`, `<Label>`, `<Textarea>`, `<Select>`). Ships as a workspace package consumed by `apps/web` and `apps/design-studio`.

## Install (workspace)

```bash
pnpm add @42ch/nexus-ui --workspace
```

## Public exports

| Entry | Description |
|-------|-------------|
| `@42ch/nexus-ui` | Brand token constants (`brandColors`, `logoVariants`, sizing guidance) + React components (`<NexusLogo>`, `<NexusMark>`, promoted UI primitives, `cn`) |
| `@42ch/nexus-ui/tokens` | Same token module (direct import) |
| `@42ch/nexus-ui/theme.css` | Brand CSS custom properties (`--nexus-brand-*`) |
| `@42ch/nexus-ui/assets/logos/logo-primary.svg` | Deep blue mark (`#1E3A5F`, flat primary) for light backgrounds |
| `@42ch/nexus-ui/assets/logos/logo-color.svg` | Cyan mark (`#25D1E0`) — bright logo for dark backgrounds |
| `@42ch/nexus-ui/assets/logos/logo-white.svg` | White mark (`#FFFFFF`) |
| `@42ch/nexus-ui/assets/logos/logo-mono.svg` | Monotone mark (`currentColor`) |

### Promoted primitives

| Component | Import | Variants | Notes |
|-----------|--------|----------|-------|
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
import { NexusLogo, NexusMark } from '@42ch/nexus-ui';

// The consumer resolves the SVG URL through its own bundler.
// In a Vite project, importing an SVG yields a URL string:
import logoPrimary from '@42ch/nexus-ui/assets/logos/logo-primary.svg';

function AppShell() {
  // Pass the resolved URL as `src`. `variant` documents which mark is being rendered.
  return <NexusLogo variant="primary" src={logoPrimary} size={32} />;
}

// Inline mono mark — inherits color via `currentColor` (no SVG asset needed):
function Badge() {
  return (
    <span style={{ color: '#1E3A5F' }}>
      <NexusMark size={24} />
    </span>
  );
}
```

**Why `<NexusLogo>` takes a `src` prop.** The package itself cannot bundle SVG assets — its build tool (`tsup` / `esbuild`) does not resolve `.svg` imports. Making SVG resolution the *consumer's* responsibility keeps the package bundler-agnostic: any consumer (Vite, webpack, Rollup, etc.) imports the SVG file through its own loader and passes the resulting URL. `<NexusMark>` sidesteps this by inlining the mono mark's path data as hand-authored JSX, so it needs no asset resolution at all. See `AGENTS.md` § *Component export strategy*.

### Raw SVG URL (without the React component)

If you want the logo asset without the component wrapper — for example, a plain `<img>` in a non-React surface — import the SVG directly:

```ts
import nexusLogo from '@42ch/nexus-ui/assets/logos/logo-primary.svg';

// <img src={nexusLogo} alt="Nexus" width={32} height={32} />
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
