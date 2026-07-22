import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';

import { EmptyState, ErrorState, LoadingState, Spinner } from './states';

/**
 * V1.121 P1 T2 — States v0.4 (DESIGN.md components.states).
 *
 * Pins:
 * - ErrorState tint/border consume the error-surface tokens (no raw
 *   color-mix arbitrary classes);
 * - EmptyState headline is content voice (`font-display text-display-24`);
 * - Spinner/LoadingState stay on the components.states tokens.
 */
describe('Spinner / LoadingState', () => {
  it('spinner uses the blue-700 token at 16px', () => {
    const { container } = render(<Spinner />);
    const icon = container.querySelector('svg');
    expect(icon).not.toBeNull();
    expect(icon!.getAttribute('class')).toMatch(/\bh-4\b/);
    expect(icon!.getAttribute('class')).toMatch(/\bw-4\b/);
    expect(icon!.getAttribute('class')).toMatch(/\btext-blue-700\b/);
  });

  it('loading state pairs the spinner with copy-14 gray-700 text', () => {
    const { container } = render(<LoadingState label="Scanning for agents…" />);
    expect(screen.getByText('Scanning for agents…')).toBeInTheDocument();
    const row = container.firstChild as HTMLElement;
    expect(row.className).toMatch(/\btext-copy-14\b/);
    expect(row.className).toMatch(/\btext-gray-700\b/);
  });
});

describe('EmptyState (content voice)', () => {
  it('headline uses the serif display tier (font-display text-display-24)', () => {
    render(<EmptyState title="No agents found on PATH" description="Install an agent below." />);
    const headline = screen.getByText('No agents found on PATH');
    expect(headline.className).toMatch(/\bfont-display\b/);
    expect(headline.className).toMatch(/\btext-display-24\b/);
    expect(headline.className).toMatch(/\btext-gray-1000\b/);
    // Interface-voice heading treatment is gone.
    expect(headline.className).not.toMatch(/\btext-heading-16\b/);
    expect(headline.className).not.toMatch(/\bfont-heading\b/);
  });
});

describe('ErrorState (error-surface tokens)', () => {
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
    const title = screen.getByText('Could not load this view');
    const description = screen.getByText('The daemon did not respond.');
    expect(title.className).toMatch(/\btext-red-1000\b/);
    expect(description.className).toMatch(/\btext-red-900\b/);
  });

  it('retry control uses deep ink on light and cyan on dark', () => {
    render(<ErrorState title="Could not load this view" onRetry={() => undefined} retryLabel="Retry" />);
    const retry = screen.getByRole('button', { name: 'Retry' });
    expect(retry.className).toMatch(/\btext-brand-deep-blue\b/);
    expect(retry.className).toMatch(/\bdark:text-blue-700\b/);
    expect(retry.className).not.toMatch(/(?<!dark:)text-blue-700\b/);
  });
});
