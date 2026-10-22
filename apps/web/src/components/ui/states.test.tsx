import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';

import { i18n } from '@/lib/i18n/config';

import { EmptyState, ErrorState, LoadingState, Spinner, UnavailableState } from './states';

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
  it('spinner uses blue-1000 on light and blue-700 on dark at 16px', () => {
    const { container } = render(<Spinner />);
    const icon = container.querySelector('svg');
    expect(icon).not.toBeNull();
    expect(icon!.getAttribute('class')).toMatch(/\bh-4\b/);
    expect(icon!.getAttribute('class')).toMatch(/\bw-4\b/);
    expect(icon!.getAttribute('class')).toMatch(/\btext-blue-1000\b/);
    expect(icon!.getAttribute('class')).toMatch(/\bdark:text-blue-700\b/);
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

describe('copy i18n defaults (V1.148 wrapper)', () => {
  it('falls back to the zh-CN translated loading / error title / retry copy when props are omitted', async () => {
    await i18n.changeLanguage('zh-CN');

    render(<LoadingState />);
    expect(screen.getByText('加载中…')).toBeInTheDocument();

    render(<ErrorState onRetry={() => undefined} />);
    expect(screen.getByText('无法加载此视图')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '重试' })).toBeInTheDocument();

    render(<UnavailableState title="服务不可用" onRetry={() => undefined} />);
    // ErrorState + UnavailableState both fell back to the same zh-CN retry copy.
    expect(screen.getAllByRole('button', { name: '重试' })).toHaveLength(2);
  });

  it('explicit copy props override the injected zh-CN defaults', async () => {
    await i18n.changeLanguage('zh-CN');

    render(<LoadingState label="Hold on…" />);
    expect(screen.getByText('Hold on…')).toBeInTheDocument();
    expect(screen.queryByText('加载中…')).not.toBeInTheDocument();

    render(
      <ErrorState title="Custom failure" onRetry={() => undefined} retryLabel="Again" />,
    );
    expect(screen.getByText('Custom failure')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Again' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '重试' })).not.toBeInTheDocument();

    render(<UnavailableState title="Nope" onRetry={() => undefined} retryLabel="Once more" />);
    const retryButtons = screen.getAllByRole('button');
    expect(retryButtons.some((b) => b.textContent === 'Once more')).toBe(true);
  });
});
