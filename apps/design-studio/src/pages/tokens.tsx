import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { useTheme } from '@/components/theme-provider';

/* ------------------------------------------------------------------ */
/*  Data — token inventory from SSOT                                    */
/* ------------------------------------------------------------------ */

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

/* ---------- Typography specimens (DESIGN.md frontmatter typography:) ----------
 *
 * Class strings are written out literally so the Tailwind scanner emits them
 * (dynamic `text-${name}` interpolation is invisible to the scanner). Metrics
 * (size / weight / line-height / tracking) are read live from the rendered
 * specimen's computed style — never hardcoded copies of the token values.
 *
 * Voice discipline (DESIGN.md §Design Concept): the display tier is the
 * content voice (Source Serif 4, `font-display`) — creative-entity titles
 * only; everything else stays interface voice (`font-sans` / `font-mono`).
 */

interface TypoSpecimen {
  label: string;
  role: string;
  /** Literal text-* size class from the shared preset. */
  textClass: string;
  /** Literal font family utility. */
  familyClass: 'font-display' | 'font-sans' | 'font-mono';
  /** Literal weight utility; display tier bakes weight 600 into text-display-*. */
  weightClass?: 'font-heading' | 'font-semibold' | 'font-medium' | 'font-button';
  sampleText: string;
}

const TYPO_SPECIMENS: TypoSpecimen[] = [
  // ── Content voice (V1.121 v0.4 display tier — Source Serif 4) ──
  { label: 'display-32', role: 'Content voice · page-level creative titles', textClass: 'text-display-32', familyClass: 'font-display', sampleText: 'The Orchard of Small Hours' },
  { label: 'display-24', role: 'Content voice · work / world titles', textClass: 'text-display-24', familyClass: 'font-display', sampleText: 'Chapter Six — The Long Descent' },
  { label: 'display-20', role: 'Content voice · card & chapter titles', textClass: 'text-display-20', familyClass: 'font-display', sampleText: 'A Field Guide to Tidal Magic' },
  // ── Interface voice (sans) ──
  { label: 'heading-32', role: 'Page / view title', textClass: 'text-heading-32', familyClass: 'font-sans', weightClass: 'font-heading', sampleText: 'Heading 32 — The quick brown fox' },
  { label: 'heading-24', role: 'Section title', textClass: 'text-heading-24', familyClass: 'font-sans', weightClass: 'font-heading', sampleText: 'Heading 24 — The quick brown fox' },
  { label: 'heading-20', role: 'Card title / dense section', textClass: 'text-heading-20', familyClass: 'font-sans', weightClass: 'font-heading', sampleText: 'Heading 20 — The quick brown fox' },
  { label: 'heading-16', role: 'Inline heading', textClass: 'text-heading-16', familyClass: 'font-sans', weightClass: 'font-heading', sampleText: 'Heading 16 — The quick brown fox' },
  { label: 'label-14', role: 'Form labels, nav items, table headers', textClass: 'text-label-14', familyClass: 'font-sans', weightClass: 'font-medium', sampleText: 'Label 14 — The quick brown fox' },
  { label: 'label-12', role: 'Badge labels, compact headers', textClass: 'text-label-12', familyClass: 'font-sans', weightClass: 'font-semibold', sampleText: 'LABEL 12 — THE QUICK BROWN FOX' },
  { label: 'copy-16', role: 'Primary body copy', textClass: 'text-copy-16', familyClass: 'font-sans', sampleText: 'Body 16 — The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs.' },
  { label: 'copy-14', role: 'Default UI copy', textClass: 'text-copy-14', familyClass: 'font-sans', sampleText: 'Body 14 — The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs.' },
  { label: 'copy-13', role: 'Dense helper text', textClass: 'text-copy-13', familyClass: 'font-sans', sampleText: 'Body 13 — The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs.' },
  { label: 'button-14', role: 'Default button label', textClass: 'text-button-14', familyClass: 'font-sans', weightClass: 'font-button', sampleText: 'Button 14 — Continue' },
  { label: 'button-12', role: 'Compact button label', textClass: 'text-button-12', familyClass: 'font-sans', weightClass: 'font-semibold', sampleText: 'BUTTON 12 — SAVE' },
  // ── Interface voice (mono) ──
  { label: 'label-12-mono', role: 'IDs, table figures, code-like values', textClass: 'text-label-12-mono', familyClass: 'font-mono', weightClass: 'font-medium', sampleText: 'mono-12 — 0xDEAD_BEEF_2024' },
  { label: 'copy-13-mono', role: 'Dense mono body', textClass: 'text-copy-13-mono', familyClass: 'font-mono', sampleText: 'mono-13 — const answer = 42; // the quick brown fox' },
];

/* ---------- Spacing scale (DESIGN.md frontmatter spacing:) ----------
 *
 * Bars render at true scale via the token's CSS custom property
 * (`width: var(--space-N)`), projected in tokens.css `:root` from the DESIGN
 * spacing: frontmatter — so the visualization tracks the SSOT, not the
 * Tailwind default scale. The px/rem readout is read live from the rendered
 * bar's computed width.
 */

interface SpacingStep {
  label: string;
  varName: string;
}

const SPACING_SCALE: SpacingStep[] = [
  { label: 'space-1', varName: '--space-1' },
  { label: 'space-2', varName: '--space-2' },
  { label: 'space-3', varName: '--space-3' },
  { label: 'space-4', varName: '--space-4' },
  { label: 'space-6', varName: '--space-6' },
  { label: 'space-8', varName: '--space-8' },
  { label: 'space-10', varName: '--space-10' },
  { label: 'space-16', varName: '--space-16' },
  { label: 'space-24', varName: '--space-24' },
];

/* ---------- Radius scale (DESIGN.md frontmatter rounded:) ---------- */

interface RadiusStep {
  label: string;
  varName: string;
}

const RADIUS_SCALE: RadiusStep[] = [
  { label: 'control', varName: '--radius-control' },
  { label: 'card', varName: '--radius-card' },
  { label: 'popover', varName: '--radius-popover' },
  { label: 'fullscreen', varName: '--radius-fullscreen' },
  { label: 'pill', varName: '--radius-pill' },
];

/* ---------- Elevation scale (DESIGN.md §Elevation, V1.121 v0.4) ---------- */

interface ElevationToken {
  label: string;
  varName: string;
  usage: string;
}

const ELEVATION_LEVELS: ElevationToken[] = [
  { label: 'elevation-0', varName: '--shadow-elevation-0', usage: 'Flat / sunk into surface' },
  { label: 'elevation-1', varName: '--shadow-elevation-1', usage: 'Resting card / canvas node at rest' },
  { label: 'elevation-2', varName: '--shadow-elevation-2', usage: 'Hover / raised (interactive lift)' },
  { label: 'elevation-3', varName: '--shadow-elevation-3', usage: 'Popover / floating (menus, tooltips, command panels)' },
  { label: 'elevation-4', varName: '--shadow-elevation-4', usage: 'Modal / dragging' },
];

/** Legacy alias chain — zero consumer breakage (DESIGN.md §Elevation). */
const ELEVATION_ALIASES = [
  { label: 'shadow-card', varName: '--shadow-card', target: 'elevation-1' },
  { label: 'shadow-popover', varName: '--shadow-popover', target: 'elevation-3' },
  { label: 'shadow-modal', varName: '--shadow-modal', target: 'elevation-4' },
] as const;

/* ---------- Motion tokens (DESIGN.md §Motion, V1.121 v0.4) ---------- */

interface MotionToken {
  label: string;
  varName: string;
  usage: string;
}

const MOTION_DURATIONS: MotionToken[] = [
  { label: 'duration-instant', varName: '--duration-instant', usage: 'Table filtering, data refresh replacement' },
  { label: 'duration-state', varName: '--duration-state', usage: 'Hover / focus / pressed states' },
  { label: 'duration-popover', varName: '--duration-popover', usage: 'Menus, dropdowns, tooltips' },
  { label: 'duration-modal', varName: '--duration-modal', usage: 'Dialog open / close' },
  { label: 'duration-enter', varName: '--duration-enter', usage: 'Entering surfaces (popover content in, toast in)' },
  { label: 'duration-exit', varName: '--duration-exit', usage: 'Dismissing surfaces (exit is faster than enter)' },
];

const MOTION_EASINGS: MotionToken[] = [
  { label: 'ease-standard', varName: '--ease-standard', usage: 'Default UI ease' },
  { label: 'ease-emphasized', varName: '--ease-emphasized', usage: 'Modal / panel enter' },
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

/**
 * Read the computed box-shadow for a shadow CSS custom property, resolving
 * through the alias chain (`--shadow-card` → `var(--shadow-elevation-1)`)
 * to the value the browser actually paints.
 */
function resolveBoxShadow(varName: string): string {
  const el = document.createElement('div');
  el.style.boxShadow = `var(${varName})`;
  el.style.display = 'none';
  document.body.appendChild(el);
  const computed = getComputedStyle(el).boxShadow;
  document.body.removeChild(el);
  return computed;
}

/**
 * Read a computed property produced by assigning a CSS custom property to a
 * probe element — live from the token's declared value, not a hardcoded copy.
 * Returns '' when the var is not defined (e.g. jsdom without CSS).
 */
function useComputedVarValue(
  varName: string,
  property: 'transitionDuration' | 'transitionTimingFunction',
): string {
  const [value, setValue] = useState('');
  useEffect(() => {
    const el = document.createElement('div');
    el.style[property] = `var(${varName})`;
    el.style.display = 'none';
    document.body.appendChild(el);
    const computed = getComputedStyle(el)[property];
    document.body.removeChild(el);
    setValue(computed ?? '');
  }, [varName, property]);
  return value;
}

/** Live prefers-reduced-motion state (drives the demo honesty note). */
function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState(false);
  useEffect(() => {
    const mql = window.matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => setReduced(mql.matches);
    update();
    mql.addEventListener('change', update);
    return () => mql.removeEventListener('change', update);
  }, []);
  return reduced;
}

/** Format a px value as rem (1rem = 16px). */
function pxToRem(px: number): string {
  return `${px / 16}rem`;
}

/** Trim a ratio to at most `decimals` places without trailing zeros. */
function trimRatio(value: number, decimals: number): string {
  return String(Number(value.toFixed(decimals)));
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
      const rafId = requestAnimationFrame(() => setComputed(resolveSwatchColor(token.varName)));
      return () => cancelAnimationFrame(rafId);
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

/**
 * Typography specimen row. Renders the specimen with the literal token
 * classes and reads font-size / weight / line-height / letter-spacing back
 * from the computed style (live) for the metrics line.
 */
function TypoRow({ specimen }: { specimen: TypoSpecimen }) {
  const specimenRef = useRef<HTMLDivElement>(null);
  const [metrics, setMetrics] = useState('');

  useEffect(() => {
    const el = specimenRef.current;
    if (!el) return;
    const cs = getComputedStyle(el);
    const fontSize = cs.fontSize ?? '';
    const sizePx = parseFloat(fontSize);
    if (!fontSize.endsWith('px') || Number.isNaN(sizePx) || sizePx === 0) {
      setMetrics('');
      return;
    }
    const parts: string[] = [fontSize];
    if (cs.fontWeight) parts.push(`weight ${cs.fontWeight}`);
    const lineHeight = cs.lineHeight ?? '';
    if (lineHeight.endsWith('px')) {
      parts.push(`line-height ${trimRatio(parseFloat(lineHeight) / sizePx, 2)} (${lineHeight})`);
    } else if (lineHeight && lineHeight !== 'normal') {
      parts.push(`line-height ${lineHeight}`);
    }
    const tracking = cs.letterSpacing ?? '';
    if (tracking.endsWith('px')) {
      parts.push(`tracking ${trimRatio(parseFloat(tracking) / sizePx, 3)}em`);
    } else if (tracking === 'normal') {
      parts.push('tracking 0');
    }
    setMetrics(parts.join(' · '));
  }, []);

  const className = [
    specimen.textClass,
    specimen.familyClass,
    specimen.weightClass ?? '',
    'text-gray-1000',
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <div
      data-testid={`typo-row-${specimen.label}`}
      className="flex flex-col sm:flex-row sm:items-baseline gap-2 py-4 border-b border-gray-alpha-200 last:border-b-0"
    >
      <div className="w-44 shrink-0 flex flex-col gap-0.5">
        <span className="text-label-14 font-medium text-gray-1000">{specimen.label}</span>
        <span className="text-label-12-mono font-mono text-gray-500">{specimen.familyClass}</span>
        <span className="text-copy-13 text-gray-600">{specimen.role}</span>
      </div>
      <div className="flex-1 min-w-0">
        <div ref={specimenRef} className={className}>
          {specimen.sampleText}
        </div>
        {metrics && (
          <div className="text-copy-13-mono font-mono text-gray-500 mt-1">{metrics}</div>
        )}
      </div>
    </div>
  );
}

/**
 * Spacing bar rendered at true scale — the bar's width is the token's CSS
 * variable itself, so what you see is the token. The px/rem readout is read
 * live from the rendered bar.
 */
function SpacingBar({ step }: { step: SpacingStep }) {
  const barRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState('');

  useEffect(() => {
    const el = barRef.current;
    if (!el) return;
    setWidth(getComputedStyle(el).width);
  }, []);

  const px = parseFloat(width);
  const hasValue = width.endsWith('px') && !Number.isNaN(px) && px > 0;

  return (
    <div data-testid={`spacing-row-${step.label}`} className="flex items-center gap-4 py-2">
      <div className="w-32 shrink-0 flex flex-col gap-0.5">
        <span className="text-label-14 font-medium text-gray-1000">{step.label}</span>
        <span className="text-copy-13-mono font-mono text-gray-500">{step.varName}</span>
      </div>
      <div className="flex-1 flex items-center gap-3">
        <div
          ref={barRef}
          className="h-6 bg-blue-700 rounded-control"
          style={{ width: `var(${step.varName})` }}
        />
        {hasValue && (
          <span className="text-copy-13 text-gray-500 font-mono shrink-0">
            {width} / {pxToRem(px)}
          </span>
        )}
      </div>
    </div>
  );
}

/** Radius swatch — the box's corner radius is the token's CSS variable. */
function RadiusBox({ step }: { step: RadiusStep }) {
  const boxRef = useRef<HTMLDivElement>(null);
  const [radius, setRadius] = useState('');

  useEffect(() => {
    const el = boxRef.current;
    if (!el) return;
    setRadius(getComputedStyle(el).borderRadius);
  }, []);

  return (
    <div data-testid={`radius-box-${step.label}`} className="flex flex-col items-center gap-3">
      <div
        ref={boxRef}
        className="w-20 h-20 bg-gray-100 border border-gray-alpha-400"
        style={{ borderRadius: `var(${step.varName})` }}
      />
      <div className="flex flex-col items-center gap-0.5">
        <span className="text-label-14 text-gray-1000">{step.label}</span>
        <span className="text-copy-13-mono font-mono text-gray-500">{step.varName}</span>
        {radius && <span className="text-copy-13 text-gray-600 font-mono">{radius}</span>}
      </div>
    </div>
  );
}

/** Elevation swatch — shadow applied via the live CSS variable. */
function ElevationCard({ token }: { token: ElevationToken }) {
  const { resolvedTheme } = useTheme();
  const [computed, setComputed] = useState<string>(() => resolveBoxShadow(token.varName));

  useEffect(() => {
    if (typeof window !== 'undefined') {
      const rafId = requestAnimationFrame(() => setComputed(resolveBoxShadow(token.varName)));
      return () => cancelAnimationFrame(rafId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedTheme, token.varName]);

  return (
    <div data-testid={`elevation-swatch-${token.label}`} className="flex flex-col gap-3">
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

/** Alias-chain row — proves the legacy name resolves onto the scale. */
function ElevationAliasRow({ alias }: { alias: (typeof ELEVATION_ALIASES)[number] }) {
  const { resolvedTheme } = useTheme();
  const [computed, setComputed] = useState<string>(() => resolveBoxShadow(alias.varName));

  useEffect(() => {
    if (typeof window !== 'undefined') {
      const rafId = requestAnimationFrame(() => setComputed(resolveBoxShadow(alias.varName)));
      return () => cancelAnimationFrame(rafId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedTheme, alias.varName]);

  return (
    <div className="flex flex-col sm:flex-row sm:items-baseline gap-1 sm:gap-3 py-2 border-b border-gray-alpha-200 last:border-b-0">
      <span className="text-label-14 font-medium text-gray-1000 w-40 shrink-0">
        {alias.label}
      </span>
      <span className="text-copy-13-mono font-mono text-gray-600 w-40 shrink-0">
        → {alias.target}
      </span>
      <span className="text-copy-13 text-gray-500 font-mono break-all">{computed}</span>
    </div>
  );
}

/** Motion token row — value read live from the token's CSS variable. */
function MotionRow({
  token,
  property,
}: {
  token: MotionToken;
  property: 'transitionDuration' | 'transitionTimingFunction';
}) {
  const value = useComputedVarValue(token.varName, property);
  return (
    <div
      data-testid={`motion-row-${token.label}`}
      className="flex flex-col sm:flex-row sm:items-baseline gap-1 sm:gap-3 py-2 border-b border-gray-alpha-200 last:border-b-0"
    >
      <span className="text-label-14 font-medium text-gray-1000 w-44 shrink-0">{token.label}</span>
      <span className="text-copy-13-mono font-mono text-gray-700 w-56 shrink-0 break-all">
        {value || token.varName}
      </span>
      <span className="text-copy-13 text-gray-600">{token.usage}</span>
    </div>
  );
}

const DEMO_BUTTON_CLASS =
  'px-3 py-1.5 rounded-control border border-gray-alpha-400 bg-background-100 text-button-14 font-button text-gray-1000 hover:bg-gray-alpha-100 transition-colors duration-state ease-standard motion-reduce:transition-none';

/**
 * Card hover-lift recipe (DESIGN.md §Motion / §Elevation): rest elevation-1,
 * hover elevation-2 + translateY(-1px) over 160ms ease-standard, pressed
 * returns to elevation-1. Reduced motion: instant state change, no
 * transform/opacity animation (motion-reduce guards).
 */
function HoverLiftDemo() {
  return (
    <div
      data-testid="motion-demo-lift"
      tabIndex={0}
      className="rounded-card border border-gray-alpha-300 bg-background-100 p-5 shadow-elevation-1 transition-all duration-popover ease-standard hover:-translate-y-px hover:shadow-elevation-2 focus-visible:-translate-y-px focus-visible:shadow-elevation-2 active:translate-y-0 active:shadow-elevation-1 motion-reduce:transition-none motion-reduce:transform-none"
    >
      <p className="text-label-14 font-medium text-gray-1000 mb-1">Card hover lift</p>
      <p className="text-copy-13 text-gray-600">
        Rest <code>elevation-1</code> → hover <code>elevation-2</code> + <code>translateY(-1px)</code>,
        160ms <code>ease-standard</code>; pressed returns to <code>elevation-1</code>.
      </p>
    </div>
  );
}

/**
 * Popover enter/exit recipe: enter opacity + scale(0.98 → 1) with
 * duration-enter (200ms) ease-standard; exit fades with duration-exit
 * (140ms). Reduced motion collapses both to an instant state change.
 */
function EnterExitDemo() {
  const [visible, setVisible] = useState(true);

  const replay = () => {
    setVisible(false);
    // Outlasts duration-exit (140ms) so the exit completes before re-enter.
    window.setTimeout(() => setVisible(true), 280);
  };

  return (
    <div>
      <div className="flex flex-wrap gap-2 mb-4">
        <button type="button" data-testid="motion-demo-replay" className={DEMO_BUTTON_CLASS} onClick={replay}>
          Replay enter
        </button>
        <button
          type="button"
          data-testid="motion-demo-dismiss"
          className={DEMO_BUTTON_CLASS}
          onClick={() => setVisible(false)}
        >
          Dismiss
        </button>
      </div>
      <div
        data-testid="motion-demo-enter-exit"
        className={[
          'rounded-popover border border-gray-alpha-300 bg-background-100 p-4 shadow-elevation-3',
          'transition-all ease-standard motion-reduce:transition-none motion-reduce:transform-none',
          visible ? 'opacity-100 scale-100 duration-enter' : 'opacity-0 scale-[0.98] duration-exit',
        ].join(' ')}
      >
        <p className="text-label-14 font-medium text-gray-1000 mb-1">Popover enter / exit</p>
        <p className="text-copy-13 text-gray-600">
          Enter: opacity + <code>scale(0.98 → 1)</code>, <code>duration-enter</code> (200ms){' '}
          <code>ease-standard</code>. Exit: opacity out, <code>duration-exit</code> (140ms).
        </p>
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
    { label: 'Motion', href: '#tokens-motion' },
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
    <section data-testid="tokens-typography">
      <SectionHeading id="tokens-typography">Typography</SectionHeading>
      <p className="text-copy-14 text-gray-700 mb-4 max-w-prose">
        The display tier (<code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">font-display</code>,
        Source Serif 4) is the <strong>content voice</strong> — creative-entity titles only, never nav,
        buttons, tables, badges, or labels. Everything else is the interface voice (sans / mono).
        Metrics are read live from each rendered specimen.
      </p>
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        {TYPO_SPECIMENS.map((s) => (
          <TypoRow key={s.label} specimen={s} />
        ))}
      </div>
    </section>
  );
}

function SpacingSection() {
  return (
    <section data-testid="tokens-spacing">
      <SectionHeading id="tokens-spacing">Spacing</SectionHeading>
      <p className="text-copy-14 text-gray-700 mb-4 max-w-prose">
        Base unit 4px. Bars render at true scale — each bar's width is the token's{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">--space-*</code> CSS
        variable, with the computed px/rem read live.
      </p>
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        {SPACING_SCALE.map((s) => (
          <SpacingBar key={s.label} step={s} />
        ))}
      </div>
    </section>
  );
}

function RadiusSection() {
  return (
    <section data-testid="tokens-radius">
      <SectionHeading id="tokens-radius">Radius</SectionHeading>
      <div className="flex flex-wrap items-end gap-8 p-6 border border-gray-alpha-300 rounded-card bg-background-100">
        {RADIUS_SCALE.map((s) => (
          <RadiusBox key={s.label} step={s} />
        ))}
      </div>
    </section>
  );
}

function ElevationSection() {
  return (
    <section data-testid="tokens-elevation">
      <SectionHeading id="tokens-elevation">Elevation</SectionHeading>
      <p className="text-copy-14 text-gray-700 mb-4 max-w-prose">
        Two-part shadows (tight ambient + soft key), ink-tinted in light, pure-black in dark.
        Swatches apply the live <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">--shadow-elevation-*</code>{' '}
        variables and re-read on theme flip.
      </p>
      <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-6">
        {ELEVATION_LEVELS.map((t) => (
          <ElevationCard key={t.varName} token={t} />
        ))}
      </div>
      <div data-testid="elevation-aliases" className="mt-6 border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <h4 className="text-heading-16 font-semibold text-gray-900 mb-2">Alias chain (no consumer breakage)</h4>
        <p className="text-copy-13 text-gray-600 mb-3">
          Legacy names resolve onto the scale. <code>elevation-2</code> has no legacy alias — consume it
          directly (<code>shadow-elevation-2</code>) for hover states.
        </p>
        {ELEVATION_ALIASES.map((a) => (
          <ElevationAliasRow key={a.varName} alias={a} />
        ))}
      </div>
    </section>
  );
}

function MotionSection() {
  const reduced = usePrefersReducedMotion();
  return (
    <section data-testid="tokens-motion">
      <SectionHeading id="tokens-motion">Motion</SectionHeading>
      <p className="text-copy-14 text-gray-700 mb-4 max-w-prose">
        Short and standard-eased (120–220ms). Durations and easings are read live from their{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">--duration-*</code> /{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">--ease-*</code> CSS
        variables. Every recipe honors{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">prefers-reduced-motion: reduce</code>{' '}
        by collapsing to an instant state change.
      </p>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6">
        <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
          <h4 className="text-heading-16 font-semibold text-gray-900 mb-2">Durations</h4>
          {MOTION_DURATIONS.map((t) => (
            <MotionRow key={t.label} token={t} property="transitionDuration" />
          ))}
        </div>
        <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
          <h4 className="text-heading-16 font-semibold text-gray-900 mb-2">Easings</h4>
          {MOTION_EASINGS.map((t) => (
            <MotionRow key={t.label} token={t} property="transitionTimingFunction" />
          ))}
        </div>
      </div>

      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <h4 className="text-heading-16 font-semibold text-gray-900 mb-4">Recipes</h4>
        {reduced && (
          <p data-testid="motion-reduced-note" className="text-copy-13 text-gray-600 mb-4">
            <code>prefers-reduced-motion: reduce</code> is active — these demos render as instant state
            changes with no transform/opacity animation.
          </p>
        )}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <HoverLiftDemo />
          <EnterExitDemo />
        </div>
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
        All scalar design scales from the DESIGN SSOT — colors, typography (incl. the display tier),
        spacing, radius, elevation, and motion. Values are read live from CSS custom properties and
        rendered utility classes, and update when the theme toggles.
      </p>
      <SubNav />

      <ColorsSection />
      <TypographySection />
      <SpacingSection />
      <RadiusSection />
      <ElevationSection />
      <MotionSection />

      <p className="text-copy-13 text-gray-500 mt-12 pt-8 border-t border-gray-alpha-200">
        Every gallery reads live values: colors, shadows, spacing, radius, and motion from CSS
        custom properties (re-resolved on theme flip where theme-dependent); typography from the
        computed style of elements carrying the token's utility class. Canvas token galleries land
        in P3.
      </p>
    </div>
  );
}
