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
    expect(badge).toHaveClass('text-green-1000');
    expect(badge.className).toContain('color-mix(in_srgb,var(--color-green-700)_16%,transparent)');
    expect(badge.className).toContain('color-mix(in_srgb,var(--color-green-700)_50%,transparent)');
  });

  it('renders soft queued with teal accent', () => {
    render(<Badge variant="queued">Queued</Badge>);
    const badge = screen.getByText('Queued');
    expect(badge).toHaveClass('text-teal-1000');
    expect(badge.className).toContain('color-mix(in_srgb,var(--color-teal-700)_50%,transparent)');
  });

  it('renders soft warning with amber accent', () => {
    render(<Badge variant="warning">Warning</Badge>);
    const badge = screen.getByText('Warning');
    expect(badge).toHaveClass('text-amber-1000');
    expect(badge.className).toContain('color-mix(in_srgb,var(--color-amber-700)_50%,transparent)');
  });

  it('renders soft error with red accent', () => {
    render(<Badge variant="error">Failed</Badge>);
    const badge = screen.getByText('Failed');
    expect(badge).toHaveClass('text-red-1000');
    expect(badge.className).toContain('color-mix(in_srgb,var(--color-red-700)_50%,transparent)');
  });

  it('renders soft preset with purple accent', () => {
    render(<Badge variant="preset">Preset</Badge>);
    const badge = screen.getByText('Preset');
    expect(badge).toHaveClass('text-purple-1000');
    expect(badge.className).toContain('color-mix(in_srgb,var(--color-purple-700)_50%,transparent)');
  });

  it.each([
    ['neutral', 'bg-gray-alpha-100', 'text-gray-900', null],
    ['running', 'color-mix(in_srgb,var(--color-green-700)_16%,transparent)', 'text-green-1000', 'color-mix(in_srgb,var(--color-green-700)_50%,transparent)'],
    ['queued', 'color-mix(in_srgb,var(--color-teal-700)_16%,transparent)', 'text-teal-1000', 'color-mix(in_srgb,var(--color-teal-700)_50%,transparent)'],
    ['warning', 'color-mix(in_srgb,var(--color-amber-700)_16%,transparent)', 'text-amber-1000', 'color-mix(in_srgb,var(--color-amber-700)_50%,transparent)'],
    ['error', 'color-mix(in_srgb,var(--color-red-700)_16%,transparent)', 'text-red-1000', 'color-mix(in_srgb,var(--color-red-700)_50%,transparent)'],
    ['preset', 'color-mix(in_srgb,var(--color-purple-700)_16%,transparent)', 'text-purple-1000', 'color-mix(in_srgb,var(--color-purple-700)_50%,transparent)'],
  ] as const)('renders soft %s with distinct hue and 16% fill / 50% border', (variant, bgClass, textClass, borderClass) => {
    render(<Badge variant={variant}>{variant}</Badge>);
    const badge = screen.getByText(variant);
    expect(badge).toHaveClass(textClass);
    expect(badge.className).toContain(bgClass);
    if (borderClass) {
      expect(badge.className).toContain(borderClass);
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
    expect(badge).toHaveClass('bg-green-700');
    expect(badge).toHaveClass('text-white');
    expect(badge).toHaveClass('dark:text-brand-deep-blue');
    expect(badge).toHaveClass('border-transparent');
  });

  it.each([
    ['queued', 'bg-teal-700'],
    ['warning', 'bg-amber-700'],
    ['error', 'bg-red-800'],
    ['preset', 'bg-purple-700'],
  ] as const)('renders solid %s with transparent border and dark deep-blue text', (variant, bg) => {
    render(
      <Badge tone="solid" variant={variant}>
        {variant}
      </Badge>,
    );
    const badge = screen.getByText(variant);
    expect(badge).toHaveClass(bg);
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

  // --- forwardRef ---

  it('forwards the ref to the underlying span element', () => {
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
