import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Badge } from './badge';

describe('Badge', () => {
  // --- variant rendering ---

  it('renders the neutral variant (default)', () => {
    render(<Badge>Neutral</Badge>);
    const badge = screen.getByText('Neutral');
    expect(badge).toHaveClass('bg-gray-alpha-100');
    expect(badge).toHaveClass('text-gray-900');
    expect(badge).toHaveClass('border-gray-alpha-300');
  });

  it('renders the running variant with green accent', () => {
    render(<Badge variant="running">Running</Badge>);
    const badge = screen.getByText('Running');
    expect(badge).toHaveClass('text-green-1000');
    expect(badge.className).toContain('color-mix(in_srgb,var(--color-green-700)_10%,transparent)');
  });

  it('renders the queued variant with teal accent', () => {
    render(<Badge variant="queued">Queued</Badge>);
    const badge = screen.getByText('Queued');
    expect(badge).toHaveClass('text-teal-1000');
    expect(badge.className).toContain('color-mix(in_srgb,var(--color-teal-700)_10%,transparent)');
  });

  it('renders the warning variant with amber accent', () => {
    render(<Badge variant="warning">Warning</Badge>);
    const badge = screen.getByText('Warning');
    expect(badge).toHaveClass('text-amber-1000');
    expect(badge.className).toContain('color-mix(in_srgb,var(--color-amber-700)_12%,transparent)');
  });

  it('renders the error variant with red accent', () => {
    render(<Badge variant="error">Failed</Badge>);
    const badge = screen.getByText('Failed');
    expect(badge).toHaveClass('text-red-1000');
    expect(badge.className).toContain('color-mix(in_srgb,var(--color-red-700)_12%,transparent)');
  });

  it('renders the preset variant with purple accent', () => {
    render(<Badge variant="preset">Preset</Badge>);
    const badge = screen.getByText('Preset');
    expect(badge).toHaveClass('text-purple-1000');
    expect(badge.className).toContain('color-mix(in_srgb,var(--color-purple-700)_10%,transparent)');
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
