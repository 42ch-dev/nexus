import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';

import { Button } from './button';

describe('Button', () => {
  it('encodes the background-driven contrast rule for the primary variant', () => {
    // Button is now a thin re-export from @42ch/nexus-ui (V1.99 P0 promotion).
    // Verify the rendered class output carries the correct light/dark text
    // contrast invariant — dark background → white text; cyan background → dark text.
    render(<Button variant="primary" size="default">Continue</Button>);
    const btn = screen.getByRole('button', { name: 'Continue' });
    expect(btn.className).toMatch(/\btext-white\b/);
    expect(btn.className).toMatch(/\bdark:text-brand-deep-blue\b/);
    expect(btn.className).not.toMatch(/\bdark:text-white\b/);
  });

  it('matches the primary variant snapshot in light mode', () => {
    const { container } = render(<Button variant="primary">Continue</Button>);
    expect(container.firstChild).toMatchSnapshot();
  });

  // No separate dark-mode snapshot: in jsdom, Tailwind's `dark:*` variant
  // classes are always present in the className attribute regardless of whether
  // `.dark` is added to document.documentElement. Both light and dark snapshots
  // render identical HTML, so a second snapshot is misleading. The light-mode
  // snapshot above already covers the full class structure including dark variants.
});
