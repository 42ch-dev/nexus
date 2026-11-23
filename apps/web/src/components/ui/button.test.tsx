import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';

import { Button } from './button';

describe('Button', () => {
  it('renders a native button with its accessible label', () => {
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
    render(<Button variant="primary">Continue</Button>);
    const btn = screen.getByRole('button', { name: 'Continue' });
    expect(btn.className).toMatch(/\bbg-blue-700\b/);
    expect(btn.className).toMatch(/\bhover:bg-blue-800\b/);
    expect(btn.className).toMatch(/\bactive:bg-blue-900\b/);
    expect(btn.className).toMatch(/\btext-brand-white\b/);
    expect(btn.className).toMatch(/\bdark:text-brand-deep-blue\b/);
  });
});
