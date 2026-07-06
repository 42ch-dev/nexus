import { describe, expect, it } from 'vitest';

import { cn } from './utils';

describe('cn', () => {
  it('keeps text-white when a custom font-size token is also present', () => {
    // Regression guard for tailwind-merge mis-classifying DESIGN.md custom
    // `text-button-*` tokens as text-color utilities.
    expect(cn('text-white text-button-14')).toMatch(/\btext-white\b/);
    expect(cn('text-white text-button-14')).toMatch(/\btext-button-14\b/);
    expect(cn('text-button-14 text-white')).toMatch(/\btext-white\b/);
    expect(cn('text-button-14 text-white')).toMatch(/\btext-button-14\b/);
  });

  it('keeps other text colors when custom font-size tokens are present', () => {
    expect(cn('text-gray-1000 text-button-12')).toMatch(/\btext-gray-1000\b/);
    expect(cn('text-gray-1000 text-button-12')).toMatch(/\btext-button-12\b/);
  });
});
