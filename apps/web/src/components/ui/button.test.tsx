import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';

import { Button } from './button';

describe('Button', () => {
  it('renders a native button with its accessible label', () => {
    // Button is a thin re-export from @42ch/nexus-ui; the consumer-visible
    // contract is a real <button> whose accessible name comes from children.
    render(<Button variant="primary" size="default">Continue</Button>);
    const btn = screen.getByRole('button', { name: 'Continue' });
    expect(btn.tagName).toBe('BUTTON');
    expect(btn).toBeEnabled();
  });

  it('applies native disabled semantics', () => {
    render(<Button disabled>Continue</Button>);
    expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled();
  });

  it('delegates rendering to the child element when asChild is set', () => {
    render(
      <Button asChild>
        <a href="/next">Continue</a>
      </Button>,
    );
    const link = screen.getByRole('link', { name: 'Continue' });
    expect(link.tagName).toBe('A');
    expect(link).toHaveAttribute('href', '/next');
  });

  it('applies the token-backed primary rest/hover/active recipe', () => {
    // jsdom runs without the built stylesheet (vitest `css: false`), so the
    // recipe is only observable through the token-backed utility classes on
    // the rendered element. Asserted minimally — rest fill, hover fill,
    // active fill, and the light/dark label pair — instead of pinning the
    // full class string, so size/layout utilities don't churn this test.
    render(<Button variant="primary">Continue</Button>);
    const btn = screen.getByRole('button', { name: 'Continue' });
    expect(btn.className).toMatch(/\bbg-blue-700\b/);
    expect(btn.className).toMatch(/\bhover:bg-blue-800\b/);
    expect(btn.className).toMatch(/\bactive:bg-blue-900\b/);
    expect(btn.className).toMatch(/\btext-brand-white\b/);
    expect(btn.className).toMatch(/\bdark:text-brand-deep-blue\b/);
  });
});
