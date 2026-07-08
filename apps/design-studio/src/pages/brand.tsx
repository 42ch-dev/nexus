import type { ReactNode } from 'react';

import {
  NexusLogo,
  NexusMark,
  logoVariants,
  logoMinSizePx,
  logoClearSpaceRatio,
  type LogoVariantName,
} from '@42ch/nexus-ui';

/* Consumer-resolved SVG assets — Vite processes these workspace-package imports
 * exactly as apps/web/src/components/brand/nexus-logo.tsx does. */
import logoPrimarySrc from '@42ch/nexus-ui/assets/logos/logo-primary.svg';
import logoColorSrc from '@42ch/nexus-ui/assets/logos/logo-color.svg';
import logoWhiteSrc from '@42ch/nexus-ui/assets/logos/logo-white.svg';
import logoMonoSrc from '@42ch/nexus-ui/assets/logos/logo-mono.svg';

/* ------------------------------------------------------------------ */
/*  Static data                                                         */
/* ------------------------------------------------------------------ */

const LOGO_SOURCES: Record<LogoVariantName, string> = {
  primary: logoPrimarySrc,
  color: logoColorSrc,
  white: logoWhiteSrc,
  mono: logoMonoSrc,
};

interface LogoDisplay {
  variant: LogoVariantName;
  label: string;
  fileName: string;
  description: string;
  /** Recommended surface per DESIGN.md §Logo Usage. */
  panelBgClass: string;
  /** Text color for the label row inside the panel. */
  panelTextClass: string;
}

const LOGO_DISPLAYS: LogoDisplay[] = [
  {
    variant: 'primary',
    label: 'Primary',
    fileName: logoVariants.primary,
    description: 'Deep blue mark — navigation and light-background shells.',
    panelBgClass: 'bg-background-100',
    panelTextClass: 'text-gray-1000',
  },
  {
    variant: 'color',
    label: 'Color',
    fileName: logoVariants.color,
    description: 'Cyan mark — bright logo for dark backgrounds / dark chrome.',
    panelBgClass: 'bg-gray-1000',
    panelTextClass: 'text-white',
  },
  {
    variant: 'white',
    label: 'White',
    fileName: logoVariants.white,
    description: 'White mark — dark hero, photography overlays, high-contrast panels.',
    panelBgClass: 'bg-gray-1000',
    panelTextClass: 'text-white',
  },
  {
    variant: 'mono',
    label: 'Mono',
    fileName: logoVariants.mono,
    description: 'Monotone mark — inline UI; inherits color via currentColor.',
    panelBgClass: 'bg-background-100',
    panelTextClass: 'text-gray-1000',
  },
];

interface ThemeCssSwatch {
  varName: string;
  hex: string;
  description: string;
}

/** Static brand identity values from `@42ch/nexus-ui/theme.css`. These are
 *  constant across themes (no `.dark` variants in that file). */
const THEME_CSS_SWATCHES: ThemeCssSwatch[] = [
  {
    varName: '--nexus-brand-deep-blue',
    hex: '#1E3A5F',
    description: 'Primary brand, actions, links, focus rings.',
  },
  {
    varName: '--nexus-brand-cyan',
    hex: '#25D1E0',
    description: 'Accent — icons, active indicators, dark-theme emphasis.',
  },
  {
    varName: '--nexus-brand-white',
    hex: '#FFFFFF',
    description: 'Text on deep blue fills, logo on dark hero surfaces.',
  },
];

/* ------------------------------------------------------------------ */
/*  Sub-components                                                      */
/* ------------------------------------------------------------------ */

function SubNav() {
  const items = [
    { label: 'Logos', href: '#brand-logos' },
    { label: 'Mark', href: '#brand-mark' },
    { label: 'Theme CSS', href: '#brand-theme-css' },
    { label: 'Clear space', href: '#brand-clear-space' },
  ];

  return (
    <nav aria-label="Brand sub-sections" className="flex flex-wrap gap-1 mb-8">
      {items.map(({ label, href }) => (
        <a
          key={href}
          href={href}
          className="px-3 py-1.5 rounded-md text-label-14 text-gray-700 hover:text-gray-1000 hover:bg-gray-alpha-100 transition-colors no-underline"
        >
          {label}
        </a>
      ))}
    </nav>
  );
}

function SectionHeading({ id, children }: { id: string; children: ReactNode }) {
  return (
    <h3 id={id} className="text-heading-20 font-semibold text-gray-1000 mb-4 pt-8 scroll-mt-16">
      {children}
    </h3>
  );
}

/* ---------- Logo grid ---------- */

function LogoCard({ display }: { display: LogoDisplay }) {
  const src = LOGO_SOURCES[display.variant];

  return (
    <div className="border border-gray-alpha-300 rounded-card bg-background-100 overflow-hidden">
      {/* Logo on recommended surface */}
      <div className={`${display.panelBgClass} p-8 flex items-center justify-center min-h-[100px]`}>
        <NexusLogo variant={display.variant} src={src} size={40} />
      </div>
      {/* Info */}
      <div className="p-4 border-t border-gray-alpha-200">
        <div className="flex items-center justify-between mb-1 gap-2">
          <span className="text-label-14 font-medium text-gray-1000">{display.label}</span>
          <code className="text-copy-13-mono text-gray-600 truncate">{display.fileName}</code>
        </div>
        <p className="text-copy-13 text-gray-600">{display.description}</p>
      </div>
    </div>
  );
}

function LogoGrid() {
  return (
    <section>
      <SectionHeading id="brand-logos">Logo variants</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        All four <code className="font-mono bg-gray-alpha-100 px-1 rounded">logoVariants</code> from{' '}
        <code className="font-mono bg-gray-alpha-100 px-1 rounded">@42ch/nexus-ui</code>,
        displayed on their recommended surfaces per DESIGN.md § Logo Usage.
      </p>
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {LOGO_DISPLAYS.map((d) => (
          <LogoCard key={d.variant} display={d} />
        ))}
      </div>
    </section>
  );
}

/* ---------- Mark ---------- */

function MarkSection() {
  return (
    <section>
      <SectionHeading id="brand-mark">Mark</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        The <code className="font-mono bg-gray-alpha-100 px-1 rounded">&lt;NexusMark&gt;</code>{' '}
        component renders an inline-SVG mark using <code className="font-mono bg-gray-alpha-100 px-1 rounded">currentColor</code>,
        so it inherits the surrounding text color. Both themes below use the same component with no props other than size.
      </p>

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 mb-6">
        {/* Light panel */}
        <div className="border border-gray-alpha-300 rounded-card bg-background-100 overflow-hidden">
          <div className="bg-background-100 p-8 flex flex-col items-center justify-center gap-3 min-h-[140px]">
            <NexusMark size={48} />
            <span className="text-copy-13 text-gray-600">Light surface — inherits gray-1000 via document text color.</span>
          </div>
          <div className="px-4 py-2 border-t border-gray-alpha-200 bg-gray-alpha-100">
            <span className="text-label-14 text-gray-700">Light theme</span>
          </div>
        </div>

        {/* Dark panel */}
        <div className="border border-gray-alpha-300 rounded-card bg-gray-1000 overflow-hidden">
          <div className="bg-gray-1000 p-8 flex flex-col items-center justify-center gap-3 min-h-[140px]">
            <NexusMark size={48} className="text-brand-cyan" />
            <span className="text-copy-13 text-gray-300">Dark surface — <code className="text-copy-13-mono bg-gray-alpha-200 px-1 rounded">text-brand-cyan</code> for accent contrast.</span>
          </div>
          <div className="px-4 py-2 border-t border-gray-alpha-200 bg-gray-900">
            <span className="text-label-14 text-gray-300">Dark theme</span>
          </div>
        </div>
      </div>

      <p className="text-copy-14 text-gray-600">
        Default size: <code className="font-mono bg-gray-alpha-100 px-1 rounded">{logoMinSizePx}px</code> (logoMinSizePx).
        The mark adapts via <code className="font-mono bg-gray-alpha-100 px-1 rounded">currentColor</code> —
        apply any Tailwind text color class or inline <code className="font-mono bg-gray-alpha-100 px-1 rounded">color</code> style.
      </p>
    </section>
  );
}

/* ---------- Theme CSS swatches ---------- */

function ThemeCssSwatches() {
  return (
    <section>
      <SectionHeading id="brand-theme-css">Theme variables</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Brand CSS custom properties exported by{' '}
        <code className="font-mono bg-gray-alpha-100 px-1 rounded">@42ch/nexus-ui/theme.css</code>.
        These are static brand-identity values (no per-theme variants).
      </p>

      <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
        {THEME_CSS_SWATCHES.map((s) => (
          <div key={s.varName} className="flex flex-col gap-3">
            <div
              className="w-full aspect-[3/2] rounded-card border border-gray-alpha-400"
              style={{ backgroundColor: s.hex }}
            />
            <div className="flex flex-col gap-0.5">
              <code className="text-label-14 font-mono text-gray-1000 break-all">{s.varName}</code>
              <span className="text-copy-13-mono text-gray-600">{s.hex}</span>
              <span className="text-copy-13 text-gray-600">{s.description}</span>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

/* ---------- Clear-space guidance ---------- */

function ClearSpaceSection() {
  const clearancePx = Math.round(logoMinSizePx * logoClearSpaceRatio);

  return (
    <section>
      <SectionHeading id="brand-clear-space">Clear space</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Per{' '}
        <a
          href="https://github.com/42ch/nexus/blob/main/DESIGN.md#logo-usage"
          target="_blank"
          rel="noopener noreferrer"
          className="text-blue-700 underline hover:opacity-80"
        >
          root DESIGN.md § Logo Usage
        </a>
        : minimum rendered height is <strong>{logoMinSizePx}px</strong> (<code className="font-mono bg-gray-alpha-100 px-1 rounded">logoMinSizePx</code>)
        and the exclusion zone is <strong>{logoClearSpaceRatio * 100}%</strong> of logo height
        (<code className="font-mono bg-gray-alpha-100 px-1 rounded">logoClearSpaceRatio = {logoClearSpaceRatio}</code>)
        on all sides.
      </p>

      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-8 flex flex-col items-center gap-4">
        {/* Visual: mark inside a dashed exclusion-zone box */}
        <div
          className="relative flex items-center justify-center"
          style={{
            width: logoMinSizePx + clearancePx * 2 + 48,
            height: logoMinSizePx + clearancePx * 2 + 48,
          }}
        >
          {/* Dashed exclusion zone */}
          <div
            className="absolute inset-0 border-2 border-dashed border-brand-cyan"
            style={{ borderRadius: '6px' }}
          />
          {/* Mark centered */}
          <NexusMark size={logoMinSizePx} />
        </div>

        <div className="flex flex-col items-center gap-1 text-center">
          <span className="text-label-14 text-gray-1000">
            Mark at {logoMinSizePx}×{logoMinSizePx}px with {logoClearSpaceRatio * 100}% clearance
          </span>
          <span className="text-copy-13 text-gray-600">
            Dashed cyan box = exclusion zone ({clearancePx}px on each side at this size).
          </span>
          <span className="text-copy-13 text-gray-500">
            Clear space scales proportionally — always maintain {logoClearSpaceRatio * 100}% of the rendered logo height.
          </span>
        </div>
      </div>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  Page                                                                */
/* ------------------------------------------------------------------ */

export function BrandPage() {
  return (
    <div className="max-w-6xl mx-auto py-8 px-4">
      <h2 className="text-heading-24 font-semibold text-gray-1000 mb-2">Brand</h2>
      <p className="text-copy-16 text-gray-700 mb-6">
        <code className="font-mono bg-gray-alpha-100 px-1 rounded">@42ch/nexus-ui</code> VI —
        logo variants, brand mark, theme.css swatches, and clear-space guidance.
        Brand-layer display only; no shadcn migration or package API changes.
      </p>
      <SubNav />

      <LogoGrid />
      <MarkSection />
      <ThemeCssSwatches />
      <ClearSpaceSection />

      <p className="text-copy-13 text-gray-500 mt-12 pt-8 border-t border-gray-alpha-200">
        All brand assets imported from <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@42ch/nexus-ui</code> —
        canonical SVG marks, hand-authored inline mark, and brand identity CSS custom properties.
        No assets are copied or reimplemented in this gallery.
      </p>
    </div>
  );
}
