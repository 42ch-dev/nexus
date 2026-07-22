import type { ReactNode } from 'react';

import {
  NexusLogo,
  NexusMark,
  NexusLogoVariant,
  logoVariants,
  logoMinSizePx,
  logoShellHeightPx,
  logoClearSpaceRatio,
  logoMarkAspectRatio,
  logoVariantPalettes,
  type LogoVariantName,
  type LogoVariantTheme,
} from '@42ch/nexus-ui';

/* Consumer-resolved SVG assets — Vite processes these workspace-package imports
 * exactly as apps/web/src/components/brand/nexus-logo.tsx does. */
import logoPrimarySrc from '@42ch/nexus-ui/assets/logos/logo-primary.svg';
import logoWhiteBgSrc from '@42ch/nexus-ui/assets/logos/logo-white-bg.svg';
import logoColorSrc from '@42ch/nexus-ui/assets/logos/logo-color.svg';
import logoWhiteSrc from '@42ch/nexus-ui/assets/logos/logo-white.svg';
import logoMonoSrc from '@42ch/nexus-ui/assets/logos/logo-mono.svg';
import logoTextSrc from '@42ch/nexus-ui/assets/logos/logo-text.svg';

import { StudioShellLogo } from '@/components/studio-shell-logo';

/* ------------------------------------------------------------------ */
/*  Static data                                                         */
/* ------------------------------------------------------------------ */

const LOGO_SOURCES: Record<LogoVariantName, string> = {
  primary: logoPrimarySrc,
  whiteBg: logoWhiteBgSrc,
  color: logoColorSrc,
  white: logoWhiteSrc,
  mono: logoMonoSrc,
  text: logoTextSrc,
};

interface LogoDisplay {
  variant: LogoVariantName;
  label: string;
  fileName: string;
  description: string;
  /** Recommended surface per DESIGN.md §Logo Usage. */
  panelBgClass: string;
  /**
   * When true, invert the img so currentColor wordmarks/marks read as white
   * on dark hero panels (img-embedded `currentColor` resolves to black).
   */
  invertForDark?: boolean;
}

const LOGO_DISPLAYS: LogoDisplay[] = [
  {
    variant: 'primary',
    label: 'Primary',
    fileName: logoVariants.primary,
    description:
      'Timeline mark — deep→cyan gradient for light nav / light shells (same plate as White-bg).',
    panelBgClass: 'bg-background-100',
  },
  {
    variant: 'whiteBg',
    label: 'White-bg',
    fileName: logoVariants.whiteBg,
    description:
      'Color mark on white/light plates — matches logo-white-bg.png (deep→cyan multi-stop).',
    panelBgClass: 'bg-white',
  },
  {
    variant: 'color',
    label: 'Color',
    fileName: logoVariants.color,
    description: 'Timeline mark — bright gradient for dark nav / dark shells.',
    panelBgClass: 'bg-[#08141C]',
  },
  {
    variant: 'white',
    label: 'White',
    fileName: logoVariants.white,
    description:
      'Dark-gray→white gradient mark — dark heroes, photography, high-contrast panels.',
    panelBgClass: 'bg-brand-deep-blue',
  },
  {
    variant: 'mono',
    label: 'Mono',
    fileName: logoVariants.mono,
    description:
      'Light-gray→black gradient mark (static). For tintable inline UI use <NexusMark>.',
    panelBgClass: 'bg-background-100',
  },
  {
    variant: 'text',
    label: 'Text',
    fileName: logoVariants.text,
    description:
      'Wordmark — lowercase nexus (currentColor). On dark heroes set color to white (inline) or invert img.',
    panelBgClass: 'bg-brand-deep-blue',
    invertForDark: true,
  },
];

const VARIANT_THEMES: LogoVariantTheme[] = [
  'elegant',
  'nature',
  'parchment',
  'scifi',
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
    hex: '#0D2B3E',
    description: 'Ink structure — titlebar fill, light text links, logo structure on light.',
  },
  {
    varName: '--nexus-brand-cyan',
    hex: '#25D1E0',
    description: 'Brand signal — shared light/dark accent (buttons, active bars, focus).',
  },
  {
    varName: '--nexus-brand-white',
    hex: '#FFFFFF',
    description: 'Text on deep fills; logo on dark hero surfaces.',
  },
];

/* ------------------------------------------------------------------ */
/*  Sub-components                                                      */
/* ------------------------------------------------------------------ */

function SubNav() {
  const items = [
    { label: 'Logos', href: '#brand-logos' },
    { label: 'Chronos', href: '#brand-chronos' },
    { label: 'Mark', href: '#brand-mark' },
    { label: 'Specimens', href: '#brand-specimens' },
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
      <div
        className={`${display.panelBgClass} p-8 flex items-center justify-center min-h-[100px]`}
      >
        <NexusLogo
          variant={display.variant}
          src={src}
          size={display.variant === 'text' ? 28 : 32}
          className={display.invertForDark ? 'brightness-0 invert' : undefined}
        />
      </div>
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
        All six <code className="font-mono bg-gray-alpha-100 px-1 rounded">logoVariants</code> from{' '}
        <code className="font-mono bg-gray-alpha-100 px-1 rounded">@42ch/nexus-ui</code>
        — timeline marks (wide aspect) plus wordmark — on their recommended surfaces per DESIGN.md §
        Logo Usage.
      </p>
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
        {LOGO_DISPLAYS.map((d) => (
          <LogoCard key={d.variant} display={d} />
        ))}
      </div>
    </section>
  );
}

/* ---------- Chronos Light / Dark mini context ---------- */

function ChronosShellMini({
  mode,
  children,
}: {
  mode: 'light' | 'dark';
  children: ReactNode;
}) {
  const isDark = mode === 'dark';
  return (
    <div
      className={`border border-gray-alpha-300 rounded-card overflow-hidden ${
        isDark ? 'bg-[#08141C]' : 'bg-background-100'
      }`}
      data-testid={`chronos-mini-${mode}`}
    >
      <div
        className={`flex h-10 items-center px-3 ${
          isDark ? 'bg-brand-deep-blue' : 'bg-brand-deep-blue'
        }`}
      >
        <span
          className={`text-label-14 font-medium ${
            isDark ? 'text-brand-cyan' : 'text-white'
          }`}
        >
          {isDark ? 'Chronos Dark' : 'Chronos Light'}
        </span>
      </div>
      <div
        className={`flex h-12 items-center px-3 border-b ${
          isDark
            ? 'border-white/10 bg-[#0D1B26]'
            : 'border-gray-alpha-200 bg-background-200'
        }`}
      >
        {children}
      </div>
      <div
        className={`min-h-[72px] p-4 ${
          isDark ? 'bg-[#08141C]' : 'bg-background-100'
        }`}
      >
        <div
          className={`h-8 rounded-control border ${
            isDark
              ? 'border-white/10 bg-[#0D1B26]'
              : 'border-gray-alpha-200 bg-background-200'
          }`}
        />
      </div>
    </div>
  );
}

function ChronosContextSection() {
  return (
    <section>
      <SectionHeading id="brand-chronos">Chronos shell placement</SectionHeading>
      <p
        data-testid="brand-chronos-note"
        className="text-copy-16 text-gray-700 mb-4"
      >
        Chronos identity: timeline mark (primary on light, color on dark), titlebar label white on
        light / cyan on dark, cyan signal chrome, deep ink structure. Product shell uses{' '}
        <strong>mark only</strong> — no wordmark in nav.
      </p>
      <p className="text-copy-16 text-gray-700 mb-6">
        Theme-aware placement:{' '}
        <code className="font-mono bg-gray-alpha-100 px-1 rounded">primary</code> on light,{' '}
        <code className="font-mono bg-gray-alpha-100 px-1 rounded">color</code> on dark. Toggle the
        Studio theme to verify live fixtures; the mini shells below show both placements at once.
      </p>

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 mb-6">
        <ChronosShellMini mode="light">
          <NexusLogo
            variant="primary"
            src={logoPrimarySrc}
            size={logoShellHeightPx}
            className="h-5 w-auto max-w-full shrink-0"
          />
        </ChronosShellMini>
        <ChronosShellMini mode="dark">
          <NexusLogo
            variant="color"
            src={logoColorSrc}
            size={logoShellHeightPx}
            className="h-5 w-auto max-w-full shrink-0"
          />
        </ChronosShellMini>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
        <div className="border border-gray-alpha-300 rounded-card overflow-hidden bg-brand-deep-blue">
          <div className="p-6 flex flex-col items-center gap-4 min-h-[120px] justify-center">
            <NexusLogo
              variant="white"
              src={logoWhiteSrc}
              size={28}
              className="h-7 w-auto"
            />
            <NexusLogo
              variant="text"
              src={logoTextSrc}
              size={22}
              className="h-[22px] w-auto brightness-0 invert"
            />
          </div>
          <div className="px-4 py-2 border-t border-white/10 bg-brand-deep-blue/80">
            <span className="text-label-14 text-white/80">
              Dark hero lockup — white mark + wordmark
            </span>
          </div>
        </div>

        <div className="border border-gray-alpha-300 rounded-card overflow-hidden bg-background-100">
          <div className="p-6 flex flex-col items-center gap-3 min-h-[120px] justify-center">
            <p className="text-copy-13 text-gray-600 text-center">
              Live shell fixtures follow the Studio theme toggle via{' '}
              <code className="font-mono bg-gray-alpha-100 px-1 rounded">StudioShellLogo</code>:
            </p>
            <StudioShellLogo />
          </div>
          <div className="px-4 py-2 border-t border-gray-alpha-200 bg-gray-alpha-100">
            <span className="text-label-14 text-gray-700">
              Theme-aware — toggle light/dark in the chrome
            </span>
          </div>
        </div>
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
        component renders the wide timeline mark as inline SVG using{' '}
        <code className="font-mono bg-gray-alpha-100 px-1 rounded">currentColor</code>. Height-driven
        sizing is <code className="font-mono bg-gray-alpha-100 px-1 rounded">w-auto</code> friendly.
      </p>

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 mb-6">
        <div className="border border-gray-alpha-300 rounded-card bg-background-100 overflow-hidden">
          <div className="bg-background-100 p-8 flex flex-col items-center justify-center gap-3 min-h-[120px]">
            <NexusMark size={32} className="w-auto text-brand-deep-blue" />
            <span className="text-copy-13 text-gray-600 text-center">
              Light surface — deep ink via text-brand-deep-blue.
            </span>
          </div>
          <div className="px-4 py-2 border-t border-gray-alpha-200 bg-gray-alpha-100">
            <span className="text-label-14 text-gray-700">Light theme</span>
          </div>
        </div>

        <div className="border border-gray-alpha-300 rounded-card bg-[#08141C] overflow-hidden">
          <div className="p-8 flex flex-col items-center justify-center gap-3 min-h-[120px]">
            <NexusMark size={32} className="w-auto text-brand-cyan" />
            <span className="text-copy-13 text-gray-300 text-center">
              Dark surface —{' '}
              <code className="text-copy-13-mono bg-gray-alpha-200 px-1 rounded">
                text-brand-cyan
              </code>
              .
            </span>
          </div>
          <div className="px-4 py-2 border-t border-white/10 bg-[#0D1B26]">
            <span className="text-label-14 text-gray-300">Dark theme</span>
          </div>
        </div>
      </div>

      <p className="text-copy-14 text-gray-600">
        Default size: <code className="font-mono bg-gray-alpha-100 px-1 rounded">{logoMinSizePx}px</code>{' '}
        height (logoMinSizePx). Aspect ≈ {logoMarkAspectRatio.toFixed(2)}:1 — do not force a 1:1 box.
      </p>
    </section>
  );
}

/* ---------- Theme specimens ---------- */

function SpecimensSection() {
  return (
    <section>
      <SectionHeading id="brand-specimens">Theme specimens</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Studio-only <code className="font-mono bg-gray-alpha-100 px-1 rounded">&lt;NexusLogoVariant&gt;</code>{' '}
        specimens driven by palette props (defaults in{' '}
        <code className="font-mono bg-gray-alpha-100 px-1 rounded">logoVariantPalettes</code>). Not a
        product theme switcher — gallery reference only.
      </p>

      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {VARIANT_THEMES.map((theme) => {
          const palette = logoVariantPalettes[theme];
          return (
            <div
              key={theme}
              className="border border-gray-alpha-300 rounded-card bg-background-100 overflow-hidden"
              data-testid={`logo-variant-${theme}`}
            >
              <div className="bg-[#08141C] p-8 flex items-center justify-center min-h-[100px]">
                <NexusLogoVariant theme={theme} size={28} className="w-auto" />
              </div>
              <div className="p-4 border-t border-gray-alpha-200">
                <span className="text-label-14 font-medium text-gray-1000 capitalize">{theme}</span>
                <div className="mt-2 flex items-center gap-2">
                  <span
                    className="inline-block h-3 w-3 rounded-full border border-gray-alpha-300"
                    style={{ backgroundColor: palette.start }}
                    title={palette.start}
                  />
                  <span
                    className="inline-block h-3 w-3 rounded-full border border-gray-alpha-300"
                    style={{ backgroundColor: palette.end }}
                    title={palette.end}
                  />
                  <code className="text-copy-13-mono text-gray-600 truncate">
                    {palette.start} → {palette.end}
                  </code>
                </div>
              </div>
            </div>
          );
        })}
      </div>
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
  const markHeight = logoMinSizePx;
  const markWidth = Math.round(markHeight * logoMarkAspectRatio);
  const clearancePx = Math.round(markHeight * logoClearSpaceRatio);

  return (
    <section>
      <SectionHeading id="brand-clear-space">Clear space</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Per{' '}
        <a
          href="https://github.com/42ch/nexus/blob/main/DESIGN.md#logo-usage"
          target="_blank"
          rel="noopener noreferrer"
          className="text-brand-deep-blue underline hover:opacity-80 dark:text-blue-700"
        >
          root DESIGN.md § Logo Usage
        </a>
        : minimum rendered height is <strong>{logoMinSizePx}px</strong> (
        <code className="font-mono bg-gray-alpha-100 px-1 rounded">logoMinSizePx</code>) and the
        exclusion zone is <strong>{logoClearSpaceRatio * 100}%</strong> of logo height on all sides.
        Marks are wide — size by height, not a square box.
      </p>

      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-8 flex flex-col items-center gap-4 overflow-x-auto">
        <div
          className="relative flex items-center justify-center"
          style={{
            width: markWidth + clearancePx * 2 + 32,
            height: markHeight + clearancePx * 2 + 32,
          }}
        >
          <div
            className="absolute inset-0 border-2 border-dashed border-brand-cyan"
            style={{ borderRadius: '6px' }}
          />
          <NexusMark size={markHeight} className="w-auto text-brand-deep-blue" />
        </div>

        <div className="flex flex-col items-center gap-1 text-center">
          <span className="text-label-14 text-gray-1000">
            Mark at {markHeight}px tall × ~{markWidth}px wide with {logoClearSpaceRatio * 100}%
            clearance
          </span>
          <span className="text-copy-13 text-gray-600">
            Dashed cyan box = exclusion zone ({clearancePx}px on each side at this size).
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
        Chronos timeline logo system (wide mark, no N-network lockup), shell placement, theme
        specimens, and clear-space guidance. Cyan is signal; deep blue is ink structure. Toggle
        light/dark to verify theme-aware shell fixtures.
      </p>
      <SubNav />

      <LogoGrid />
      <ChronosContextSection />
      <MarkSection />
      <SpecimensSection />
      <ThemeCssSwatches />
      <ClearSpaceSection />

      <p className="text-copy-13 text-gray-500 mt-12 pt-8 border-t border-gray-alpha-200">
        Assets from <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@42ch/nexus-ui</code>
        — SVG marks, inline <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">NexusMark</code>
        , and palette-driven{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">NexusLogoVariant</code>{' '}
        specimens. No assets are copied or reimplemented in this gallery.
      </p>
    </div>
  );
}
