import { describe, expect, it } from 'vitest';

import { isWorkShellRoute } from './work-shell-routes';

describe('isWorkShellRoute', () => {
  it('matches work-scoped outline and chapters routes', () => {
    expect(isWorkShellRoute('/works/work-a')).toBe(true);
    expect(isWorkShellRoute('/works/work-a/')).toBe(true);
    expect(isWorkShellRoute('/works/work-a/outline')).toBe(true);
    expect(isWorkShellRoute('/works/work-a/chapters')).toBe(true);
    expect(isWorkShellRoute('/works/work-a/chapters/ch-1')).toBe(true);
  });

  it('does not match the reserved /works/chapters sibling list route', () => {
    expect(isWorkShellRoute('/works/chapters')).toBe(false);
    expect(isWorkShellRoute('/works/chapters/')).toBe(false);
  });

  it('does not match non-work routes', () => {
    expect(isWorkShellRoute('/works')).toBe(false);
    expect(isWorkShellRoute('/worlds')).toBe(false);
    expect(isWorkShellRoute('/memory')).toBe(false);
  });
});
