import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Badge } from './badge';

describe('Badge', () => {
  // --- soft tone (default) ---

  it('defaults to soft tone with strengthened neutral border', () => {
    render(<Badge>Neutral</Badge>);
    const badge = screen.getByText('Neutral');
    expect(badge).toHaveClass('bg-gray-alpha-100');
    expect(badge).toHaveClass('text-gray-900');
    expect(badge).toHaveClass('border-gray-alpha-400');
  });

  it('renders soft running with ~16% fill and ~50% border alpha', () => {
    render(<Badge variant="running">Running</Badge>);
    const badge = screen.getByText('Running');
    // v1.183 P0 R-V1121P1QC1-S001: soft triples consume the projected
    // nexus-ui-badge-soft-* tokens (16% fill / -1000 text / 50% border).
    expect(badge).toHaveClass('text-nexus-ui-badge-soft-running-text');
    expect(badge).toHaveClass('bg-nexus-ui-badge-soft-running-bg');
    expect(badge).toHaveClass('border-nexus-ui-badge-soft-running-border');
  });

  it('renders soft queued with teal accent', () => {
    render(<Badge variant="queued">Queued</Badge>);
    const badge = screen.getByText('Queued');
    expect(badge).toHaveClass('text-nexus-ui-badge-soft-queued-text');
    expect(badge).toHaveClass('border-nexus-ui-badge-soft-queued-border');
  });

  it('renders soft warning with amber accent', () => {
    render(<Badge variant="warning">Warning</Badge>);
    const badge = screen.getByText('Warning');
    expect(badge).toHaveClass('text-nexus-ui-badge-soft-warning-text');
    expect(badge).toHaveClass('border-nexus-ui-badge-soft-warning-border');
  });

  it('renders soft error with red accent', () => {
    render(<Badge variant="error">Failed</Badge>);
    const badge = screen.getByText('Failed');
    expect(badge).toHaveClass('text-nexus-ui-badge-soft-error-text');
    expect(badge).toHaveClass('border-nexus-ui-badge-soft-error-border');
  });

  it('renders soft preset with purple accent', () => {
    render(<Badge variant="preset">Preset</Badge>);
    const badge = screen.getByText('Preset');
    expect(badge).toHaveClass('text-nexus-ui-badge-soft-preset-text');
    expect(badge).toHaveClass('border-nexus-ui-badge-soft-preset-border');
  });

  it.each([
    ['neutral', 'bg-gray-alpha-100', 'text-gray-900', null],
    ['running', 'bg-nexus-ui-badge-soft-running-bg', 'text-nexus-ui-badge-soft-running-text', 'border-nexus-ui-badge-soft-running-border'],
    ['queued', 'bg-nexus-ui-badge-soft-queued-bg', 'text-nexus-ui-badge-soft-queued-text', 'border-nexus-ui-badge-soft-queued-border'],
    ['warning', 'bg-nexus-ui-badge-soft-warning-bg', 'text-nexus-ui-badge-soft-warning-text', 'border-nexus-ui-badge-soft-warning-border'],
    ['error', 'bg-nexus-ui-badge-soft-error-bg', 'text-nexus-ui-badge-soft-error-text', 'border-nexus-ui-badge-soft-error-border'],
    ['preset', 'bg-nexus-ui-badge-soft-preset-bg', 'text-nexus-ui-badge-soft-preset-text', 'border-nexus-ui-badge-soft-preset-border'],
  ] as const)('renders soft %s with distinct hue and 16% fill / 50% border', (variant, bgClass, textClass, borderClass) => {
    render(<Badge variant={variant}>{variant}</Badge>);
    const badge = screen.getByText(variant);
    expect(badge).toHaveClass(textClass);
    expect(badge).toHaveClass(bgClass);
    if (borderClass) {
      expect(badge).toHaveClass(borderClass);
    }
  });

  it('explicit tone=soft matches default soft classes', () => {
    render(
      <Badge tone="soft" variant="neutral">
        Soft
      </Badge>,
    );
    const badge = screen.getByText('Soft');
    expect(badge).toHaveClass('border-gray-alpha-400');
    expect(badge).not.toHaveClass('border-transparent');
  });

  // --- solid tone ---

  it('renders solid neutral with white text and transparent border', () => {
    render(
      <Badge tone="solid" variant="neutral">
        Solid Neutral
      </Badge>,
    );
    const badge = screen.getByText('Solid Neutral');
    expect(badge).toHaveClass('bg-gray-1000');
    expect(badge).toHaveClass('text-white');
    expect(badge).toHaveClass('border-transparent');
    expect(badge).toHaveClass('dark:bg-gray-200');
  });

  it('renders solid semantic fills with dark AA text override', () => {
    render(
      <Badge tone="solid" variant="running">
        Solid Running
      </Badge>,
    );
    const badge = screen.getByText('Solid Running');
    // v1.183 P0 R-V1121P1T3-S001 (AR-3): light solid fill sits one step
    // darker (green-800) so white text clears AA; dark pins the bright
    // green-700 fill per the Button Contrast Invariant.
    expect(badge).toHaveClass('bg-green-800');
    expect(badge).toHaveClass('dark:bg-green-700');
    expect(badge).toHaveClass('text-white');
    expect(badge).toHaveClass('dark:text-brand-deep-blue');
    expect(badge).toHaveClass('border-transparent');
  });

  it.each([
    ['queued', 'bg-teal-800', 'dark:bg-teal-700'],
    ['warning', 'bg-amber-800', 'dark:bg-amber-700'],
    ['error', 'bg-red-800', null],
    ['preset', 'bg-purple-700', null],
  ] as const)('renders solid %s with transparent border and dark deep-blue text', (variant, bg, darkBg) => {
    render(
      <Badge tone="solid" variant={variant}>
        {variant}
      </Badge>,
    );
    const badge = screen.getByText(variant);
    expect(badge).toHaveClass(bg);
    if (darkBg) {
      expect(badge).toHaveClass(darkBg);
    }
    expect(badge).toHaveClass('text-white');
    expect(badge).toHaveClass('dark:text-brand-deep-blue');
    expect(badge).toHaveClass('border-transparent');
  });

  // --- base structural classes ---

  it('renders with base structural classes', () => {
    render(<Badge>Tag</Badge>);
    const badge = screen.getByText('Tag');
    expect(badge).toHaveClass('inline-flex');
    expect(badge).toHaveClass('rounded-pill');
    expect(badge).toHaveClass('h-6');
    expect(badge).toHaveClass('text-label-12');
    expect(badge).toHaveClass('font-semibold');
    expect(badge).toHaveClass('whitespace-nowrap');
  });

  it('eases state changes over duration-state (v0.4 motion tokens)', () => {
    render(<Badge>Motion</Badge>);
    const badge = screen.getByText('Motion');
    expect(badge).toHaveClass('transition-colors');
    expect(badge).toHaveClass('duration-state');
    expect(badge).toHaveClass('ease-standard');
  });

  // --- className merge (cn integration) ---

  it('merges custom className with variant classes', () => {
    render(
      <Badge variant="neutral" className="ml-2">
        Extra
      </Badge>,
    );
    const badge = screen.getByText('Extra');
    expect(badge).toHaveClass('ml-2');
    expect(badge).toHaveClass('bg-gray-alpha-100');
  });

  // --- ref-as-prop ---

  it('passes the ref to the underlying span element', () => {
    let ref: HTMLSpanElement | null = null;
    const setRef = (el: HTMLSpanElement | null) => {
      ref = el;
    };
    render(<Badge ref={setRef}>Ref</Badge>);
    expect(ref).not.toBeNull();
    expect(ref!).toHaveProperty('tagName', 'SPAN');
  });

  // --- renders children as text ---

  it('renders its children', () => {
    render(<Badge variant="error">Error Text</Badge>);
    expect(screen.getByText('Error Text')).toBeInTheDocument();
  });
});
