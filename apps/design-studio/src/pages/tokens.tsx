import { useEffect, useState, type CSSProperties, type ReactNode } from 'react';
import { useTheme } from '@/components/theme-provider';

/* ------------------------------------------------------------------ */
/*  Data — token inventory from SSOT                                    */
/* ------------------------------------------------------------------ */

/** Read a CSS custom property value from the document root. */
function readCSSVar(name: string): string {
  if (typeof window === 'undefined') return '';
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

interface ColorToken {
  label: string;
  varName: string;
}

interface TokenGroup {
  title: string;
  tokens: ColorToken[];
}

const COLOR_GROUPS: TokenGroup[] = [
  {
    title: 'Brand',
    tokens: [
      { label: 'brand-deep-blue', varName: '--color-brand-deep-blue' },
      { label: 'brand-cyan', varName: '--color-brand-cyan' },
      { label: 'brand-white', varName: '--color-brand-white' },
    ],
  },
  {
    title: 'Background',
    tokens: [
      { label: 'background-100', varName: '--color-background-100' },
      { label: 'background-200', varName: '--color-background-200' },
      { label: 'background-300', varName: '--color-background-300' },
    ],
  },
  {
    title: 'Gray (solid)',
    tokens: [
      { label: 'gray-100', varName: '--color-gray-100' },
      { label: 'gray-200', varName: '--color-gray-200' },
      { label: 'gray-300', varName: '--color-gray-300' },
      { label: 'gray-400', varName: '--color-gray-400' },
      { label: 'gray-500', varName: '--color-gray-500' },
      { label: 'gray-600', varName: '--color-gray-600' },
      { label: 'gray-700', varName: '--color-gray-700' },
      { label: 'gray-800', varName: '--color-gray-800' },
      { label: 'gray-900', varName: '--color-gray-900' },
      { label: 'gray-1000', varName: '--color-gray-1000' },
    ],
  },
  {
    title: 'Gray alpha',
    tokens: [
      { label: 'gray-alpha-100', varName: '--color-gray-alpha-100' },
      { label: 'gray-alpha-200', varName: '--color-gray-alpha-200' },
      { label: 'gray-alpha-300', varName: '--color-gray-alpha-300' },
      { label: 'gray-alpha-400', varName: '--color-gray-alpha-400' },
      { label: 'gray-alpha-500', varName: '--color-gray-alpha-500' },
      { label: 'gray-alpha-600', varName: '--color-gray-alpha-600' },
    ],
  },
  {
    title: 'Blue',
    tokens: ['700', '800', '900', '1000'].map((s) => ({
      label: `blue-${s}`,
      varName: `--color-blue-${s}`,
    })),
  },
  {
    title: 'Red',
    tokens: ['700', '800', '900', '1000'].map((s) => ({
      label: `red-${s}`,
      varName: `--color-red-${s}`,
    })),
  },
  {
    title: 'Amber',
    tokens: ['700', '800', '900', '1000'].map((s) => ({
      label: `amber-${s}`,
      varName: `--color-amber-${s}`,
    })),
  },
  {
    title: 'Green',
    tokens: ['700', '800', '900', '1000'].map((s) => ({
      label: `green-${s}`,
      varName: `--color-green-${s}`,
    })),
  },
  {
    title: 'Teal',
    tokens: ['700', '800', '900', '1000'].map((s) => ({
      label: `teal-${s}`,
      varName: `--color-teal-${s}`,
    })),
  },
  {
    title: 'Purple',
    tokens: ['700', '800', '900', '1000'].map((s) => ({
      label: `purple-${s}`,
      varName: `--color-purple-${s}`,
    })),
  },
  {
    title: 'Pink',
    tokens: ['700', '800', '900', '1000'].map((s) => ({
      label: `pink-${s}`,
      varName: `--color-pink-${s}`,
    })),
  },
];

/* ---------- Typography specimens (DESIGN.md frontmatter) ---------- */

interface TypoSpecimen {
  label: string;
  role: string;
  className: string;
  sampleText: string;
}

const TYPO_SPECIMENS: TypoSpecimen[] = [
  { label: 'heading-32', role: 'Page / view title', className: 'heading-32', sampleText: 'Heading 32 — The quick brown fox' },
  { label: 'heading-24', role: 'Section title', className: 'heading-24', sampleText: 'Heading 24 — The quick brown fox' },
  { label: 'heading-20', role: 'Card title / dense section', className: 'heading-20', sampleText: 'Heading 20 — The quick brown fox' },
  { label: 'heading-16', role: 'Inline heading', className: 'heading-16', sampleText: 'Heading 16 — The quick brown fox' },
  { label: 'label-14', role: 'Form labels, nav items, table headers', className: 'label-14', sampleText: 'Label 14 — The quick brown fox' },
  { label: 'label-12', role: 'Badge labels, compact headers', className: 'label-12', sampleText: 'LABEL 12 — THE QUICK BROWN FOX' },
  { label: 'copy-16', role: 'Primary body copy', className: 'copy-16', sampleText: 'Body 16 — The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs.' },
  { label: 'copy-14', role: 'Default UI copy', className: 'copy-14', sampleText: 'Body 14 — The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs.' },
  { label: 'copy-13', role: 'Dense helper text', className: 'copy-13', sampleText: 'Body 13 — The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs.' },
  { label: 'button-14', role: 'Default button label', className: 'button-14', sampleText: 'Button 14 — Continue' },
  { label: 'button-12', role: 'Compact button label', className: 'button-12', sampleText: 'BUTTON 12 — SAVE' },
  { label: 'label-12-mono', role: 'IDs, table figures, code-like values', className: 'label-12-mono', sampleText: 'mono-12 — 0xDEAD_BEEF_2024' },
  { label: 'copy-13-mono', role: 'Dense mono body', className: 'copy-13-mono', sampleText: 'mono-13 — const answer = 42; // the quick brown fox' },
];

/* ---------- Spacing scale (DESIGN.md frontmatter) ---------- */

interface SpacingStep {
  label: string;
  px: number;
}

const SPACING_SCALE: SpacingStep[] = [
  { label: 'base / space-1', px: 4 },
  { label: 'space-2', px: 8 },
  { label: 'space-3', px: 12 },
  { label: 'space-4', px: 16 },
  { label: 'space-6', px: 24 },
  { label: 'space-8', px: 32 },
  { label: 'space-10', px: 40 },
  { label: 'space-16', px: 64 },
  { label: 'space-24', px: 96 },
];

/* ---------- Rounded scale (DESIGN.md frontmatter) ---------- */

interface RoundedStep {
  label: string;
  radius: string;
}

const ROUNDED_SCALE: RoundedStep[] = [
  { label: 'control', radius: '6px' },
  { label: 'card', radius: '8px' },
  { label: 'popover', radius: '12px' },
  { label: 'fullscreen', radius: '16px' },
  { label: 'pill', radius: '9999px' },
];

/* ---------- Elevation tokens (tokens.css CSS vars) ---------- */

interface ElevationToken {
  label: string;
  varName: string;
  usage: string;
}

const ELEVATION_TOKENS: ElevationToken[] = [
  { label: 'shadow-card', varName: '--shadow-card', usage: 'Raised dashboard cards' },
  { label: 'shadow-popover', varName: '--shadow-popover', usage: 'Menus, tooltips, command panels' },
  { label: 'shadow-modal', varName: '--shadow-modal', usage: 'Dialogs and blocking overlays' },
];

/* ------------------------------------------------------------------ */
/*  Helpers                                                             */
/* ------------------------------------------------------------------ */

/**
 * Read the computed background-color that results from applying a CSS
 * custom property to a DOM element.  This resolves *through* var()
 * chains (e.g. `var(--nexus-brand-deep-blue)`) and returns the final
 * rgb / rgba string the browser paints.
 */
function resolveSwatchColor(varName: string): string {
  const el = document.createElement('div');
  el.style.backgroundColor = `var(${varName})`;
  el.style.display = 'none';
  document.body.appendChild(el);
  const computed = getComputedStyle(el).backgroundColor;
  document.body.removeChild(el);
  return computed;
}

/** Format a px value as rem (1rem = 16px). */
function pxToRem(px: number): string {
  return `${px / 16}rem`;
}

/* ------------------------------------------------------------------ */
/*  Sub-components                                                      */
/* ------------------------------------------------------------------ */

function SectionHeading({ id, children }: { id: string; children: ReactNode }) {
  return (
    <h3 id={id} className="text-heading-20 font-semibold text-gray-1000 mb-4 pt-8 scroll-mt-16">
      {children}
    </h3>
  );
}

function ColorSwatch({ token }: { token: ColorToken }) {
  const { resolvedTheme } = useTheme();
  const [computed, setComputed] = useState<string>(() => resolveSwatchColor(token.varName));

  useEffect(() => {
    if (typeof window !== 'undefined') {
      // Defer to next frame so CSS vars have been swapped.
      requestAnimationFrame(() => setComputed(resolveSwatchColor(token.varName)));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedTheme, token.varName]);

  return (
    <div className="flex flex-col gap-2">
      <div
        className="w-full aspect-[3/2] rounded-card border border-gray-alpha-400"
        style={{ backgroundColor: `var(${token.varName})` }}
      />
      <div className="flex flex-col gap-0.5 min-w-0">
        <span className="text-label-14 text-gray-1000 truncate">{token.label}</span>
        <span className="text-copy-13 text-gray-700 truncate font-mono">{computed}</span>
      </div>
    </div>
  );
}

function TypoRow({ specimen }: { specimen: TypoSpecimen }) {
  return (
    <div className="flex flex-col sm:flex-row sm:items-baseline gap-2 py-4 border-b border-gray-alpha-200 last:border-b-0">
      <div className="w-32 shrink-0 flex flex-col gap-0.5">
        <span className="text-label-14 font-medium text-gray-1000">{specimen.label}</span>
        <span className="text-copy-13 text-gray-600">{specimen.role}</span>
      </div>
      <div
        style={{ fontFamily: specimen.className.includes('mono') ? 'var(--font-mono)' : 'var(--font-sans)' }}
        className={`text-${specimen.className} text-gray-1000 leading-normal`}
      >
        {specimen.sampleText}
      </div>
    </div>
  );
}

function SpacingBar({ step, maxPx }: { step: SpacingStep; maxPx: number }) {
  const widthPercent = (step.px / maxPx) * 100;
  return (
    <div className="flex items-center gap-4 py-2">
      <div className="w-32 shrink-0 flex flex-col gap-0.5">
        <span className="text-label-14 font-medium text-gray-1000">{step.label}</span>
        <span className="text-copy-13 text-gray-600 font-mono">{step.px}px / {pxToRem(step.px)}</span>
      </div>
      <div className="flex-1 flex items-center gap-3">
        <div
          className="h-6 bg-blue-700 rounded-control min-w-[4px]"
          style={{ width: `${widthPercent}%` }}
        />
        <span className="text-copy-13 text-gray-500 font-mono shrink-0">{step.px}px</span>
      </div>
    </div>
  );
}

function RoundedBox({ step }: { step: RoundedStep }) {
  return (
    <div className="flex flex-col items-center gap-3">
      <div
        className="w-20 h-20 bg-gray-100 border border-gray-alpha-400"
        style={{ borderRadius: step.radius }}
      />
      <div className="flex flex-col items-center gap-0.5">
        <span className="text-label-14 text-gray-1000">{step.label}</span>
        <span className="text-copy-13 text-gray-600 font-mono">{step.radius}</span>
      </div>
    </div>
  );
}

function ElevationCard({ token }: { token: ElevationToken }) {
  const { resolvedTheme } = useTheme();
  const [computed, setComputed] = useState<string>(() => readCSSVar(token.varName));

  useEffect(() => {
    if (typeof window !== 'undefined') {
      requestAnimationFrame(() => setComputed(readCSSVar(token.varName)));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedTheme, token.varName]);

  return (
    <div className="flex flex-col gap-3">
      <div
        className="w-full aspect-[16/10] rounded-card bg-background-100 border border-gray-alpha-200 flex items-center justify-center"
        style={{ boxShadow: `var(${token.varName})` } as CSSProperties}
      >
        <span className="text-copy-14 text-gray-500 font-mono">{token.label}</span>
      </div>
      <div className="flex flex-col gap-0.5">
        <span className="text-label-14 text-gray-1000">{token.label}</span>
        <span className="text-copy-13 text-gray-600">{token.usage}</span>
        <span className="text-copy-13 text-gray-500 font-mono break-all">{computed}</span>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Sections                                                            */
/* ------------------------------------------------------------------ */

function SubNav() {
  const items = [
    { label: 'Colors', href: '#tokens-colors' },
    { label: 'Type', href: '#tokens-typography' },
    { label: 'Space', href: '#tokens-spacing' },
    { label: 'Radius', href: '#tokens-radius' },
    { label: 'Elevation', href: '#tokens-elevation' },
  ];

  return (
    <nav aria-label="Token sub-sections" className="flex flex-wrap gap-1 mb-8">
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

function ColorsSection() {
  return (
    <section>
      <SectionHeading id="tokens-colors">Colors</SectionHeading>
      {COLOR_GROUPS.map((group) => (
        <div key={group.title} className="mb-8">
          <h4 className="text-heading-16 font-semibold text-gray-900 mb-4">{group.title}</h4>
          <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
            {group.tokens.map((t) => (
              <ColorSwatch key={t.varName} token={t} />
            ))}
          </div>
        </div>
      ))}
    </section>
  );
}

function TypographySection() {
  return (
    <section>
      <SectionHeading id="tokens-typography">Typography</SectionHeading>
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        {TYPO_SPECIMENS.map((s) => (
          <TypoRow key={s.label} specimen={s} />
        ))}
      </div>
    </section>
  );
}

function SpacingSection() {
  const maxPx = SPACING_SCALE[SPACING_SCALE.length - 1].px;
  return (
    <section>
      <SectionHeading id="tokens-spacing">Spacing</SectionHeading>
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        {SPACING_SCALE.map((s) => (
          <SpacingBar key={s.label} step={s} maxPx={maxPx} />
        ))}
      </div>
    </section>
  );
}

function RadiusSection() {
  return (
    <section>
      <SectionHeading id="tokens-radius">Radius</SectionHeading>
      <div className="flex flex-wrap items-end gap-8 p-6 border border-gray-alpha-300 rounded-card bg-background-100">
        {ROUNDED_SCALE.map((s) => (
          <RoundedBox key={s.label} step={s} />
        ))}
      </div>
    </section>
  );
}

function ElevationSection() {
  return (
    <section>
      <SectionHeading id="tokens-elevation">Elevation</SectionHeading>
      <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-6">
        {ELEVATION_TOKENS.map((t) => (
          <ElevationCard key={t.varName} token={t} />
        ))}
      </div>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  Page                                                                */
/* ------------------------------------------------------------------ */

export function TokensPage() {
  return (
    <div className="max-w-6xl mx-auto py-8 px-4">
      <h2 className="text-heading-24 font-semibold text-gray-1000 mb-2">Tokens</h2>
      <p className="text-copy-16 text-gray-700 mb-6">
        All scalar design scales from the DESIGN SSOT — colors, typography, spacing, radius, and elevation.
        Values are read live from CSS custom properties and update when the theme toggles.
      </p>
      <SubNav />

      <ColorsSection />
      <TypographySection />
      <SpacingSection />
      <RadiusSection />
      <ElevationSection />

      {/* Motion — omitted: DESIGN.md §Motion describes durations/easing but
          they are not projected to CSS custom properties in tokens.css. */}
      <p className="text-copy-13 text-gray-500 mt-12 pt-8 border-t border-gray-alpha-200">
        Motion tokens (durations, easing curves) are defined in DESIGN.md §Motion but are not
        exposed as CSS custom properties in the shared tokens layer. They are consumed via
        the Tailwind preset (<code>transitionDuration</code>, <code>transitionTimingFunction</code>).
      </p>
    </div>
  );
}
