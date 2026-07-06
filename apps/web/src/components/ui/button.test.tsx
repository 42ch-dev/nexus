import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';

import { Button, buttonVariants } from './button';

describe('Button', () => {
  it('encodes the background-driven contrast rule for the primary variant', () => {
    // Test the source class from cva directly. tailwind-merge (used by `cn` at
    // render time) does not recognise custom `text-button-*` font-size tokens by
    // default, so `text-white` is dropped from the rendered class; the source
    // variant string is still the SSOT for the rule.
    const className = buttonVariants({ variant: 'primary', size: 'default' });

    // Dark background (bg-blue-700) → light/white text.
    expect(className).toMatch(/\btext-white\b/);
    // Light/bright background (dark:bg-brand-cyan) → dark text.
    expect(className).toMatch(/\bdark:text-brand-deep-blue\b/);
    // Regression guard: the cyan fill must never use white text again.
    expect(className).not.toMatch(/\bdark:text-white\b/);
  });

  it('matches the primary variant snapshot in light mode', () => {
    const { container } = render(<Button variant="primary">Continue</Button>);
    expect(container.firstChild).toMatchSnapshot();
  });

  it('matches the primary variant snapshot in dark mode', () => {
    document.documentElement.classList.add('dark');
    const { container } = render(<Button variant="primary">Continue</Button>);
    expect(container.firstChild).toMatchSnapshot();
    document.documentElement.classList.remove('dark');
  });
});
