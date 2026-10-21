import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

import {
  EmptyState,
  ErrorState,
  LoadingState,
  Spinner,
  UnavailableState,
} from './states';

/**
 * V1.148 P1 — promoted States primitives (DESIGN.md components.states).
 *
 * Pins: caller-owned copy (labels/titles/retry), the v0.4 token recipes
 * (Spinner blue-1000/blue-700 dark hook, ErrorState error-surface tokens,
 * EmptyState content voice), retry callbacks, and default fallback copy
 * matching the app `en` locale. Theme parity comes from `--nexus-*` token
 * projections and explicit `dark:` hooks.
 */
describe('Spinner', () => {
  it('uses blue-1000 on light and blue-700 on dark at 16px', () => {
    const { container } = render(<Spinner />);
    const icon = container.querySelector('svg');
    expect(icon).not.toBeNull();
    expect(icon!.getAttribute('class')).toMatch(/\bh-4\b/);
    expect(icon!.getAttribute('class')).toMatch(/\bw-4\b/);
    expect(icon!.getAttribute('class')).toMatch(/\btext-blue-1000\b/);
    expect(icon!.getAttribute('class')).toMatch(/\bdark:text-blue-700\b/);
    expect(icon!.getAttribute('class')).toMatch(/\banimate-spin\b/);
  });
});

describe('LoadingState', () => {
  it('pairs the spinner with copy-14 gray-700 caller-owned copy', () => {
    const { container } = render(<LoadingState label="Scanning for agents…" />);
    expect(screen.getByText('Scanning for agents…')).toBeInTheDocument();
    const row = container.firstChild as HTMLElement;
    expect(row.className).toMatch(/\btext-copy-14\b/);
    expect(row.className).toMatch(/\btext-gray-700\b/);
  });
});

describe('EmptyState', () => {
  it('headline uses the content voice (font-display text-display-24) and renders action slot', () => {
    render(
      <EmptyState
        title="No agents found on PATH"
        description="Install an agent below."
        action={<button>Create agent</button>}
      />,
    );
    const headline = screen.getByText('No agents found on PATH');
    expect(headline.className).toMatch(/\bfont-display\b/);
    expect(headline.className).toMatch(/\btext-display-24\b/);
    expect(headline.className).toMatch(/\btext-gray-1000\b/);
    expect(headline.className).not.toMatch(/\btext-heading-16\b/);
    expect(screen.getByRole('button', { name: 'Create agent' })).toBeInTheDocument();
  });
});

describe('UnavailableState', () => {
  it('renders neutral tone with an optional retry callback and caller-owned copy', () => {
    const onRetry = vi.fn();
    render(
      <UnavailableState title="Engine idle" description="Start the engine." onRetry={onRetry} retryLabel="Retry" />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });
});

describe('ErrorState', () => {
  it('surface consumes error-surface tokens — no raw color-mix arbitrary values', () => {
    const { container } = render(<ErrorState title="Could not load this view" />);
    const alert = container.querySelector('[role="alert"]') as HTMLElement;
    expect(alert).not.toBeNull();
    expect(alert.className).toMatch(/\bbg-error-surface\b/);
    expect(alert.className).toMatch(/\bborder-error-surface-border\b/);
    expect(alert.className).not.toMatch(/color-mix/);
  });

  it('keeps the alert role and red-1000/red-900 text recipe', () => {
    render(<ErrorState title="Could not load this view" description="The daemon did not respond." />);
    expect(screen.getByText('Could not load this view').className).toMatch(/\btext-red-1000\b/);
    expect(screen.getByText('The daemon did not respond.').className).toMatch(/\btext-red-900\b/);
  });

  it('retry control uses deep ink on light and cyan on dark', () => {
    const onRetry = vi.fn();
    render(<ErrorState title="Could not load this view" onRetry={onRetry} retryLabel="Retry" />);
    const retry = screen.getByRole('button', { name: 'Retry' });
    expect(retry.className).toMatch(/\btext-brand-deep-blue\b/);
    expect(retry.className).toMatch(/\bdark:text-blue-700\b/);
    expect(retry.className).not.toMatch(/(?<!dark:)text-blue-700\b/);
    fireEvent.click(retry);
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('falls back to default copy matching the app en locale when callers omit it', () => {
    const onRetry = vi.fn();
    render(<ErrorState onRetry={onRetry} />);
    expect(screen.getByText('Could not load this view')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });
});
