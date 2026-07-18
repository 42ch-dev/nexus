import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';

import { FindingStatusBadge } from './status-badge';

/**
 * V1.121 P1 T3 — FindingStatusBadge tokenization (DESIGN.md
 * components.finding-status-pill).
 *
 * Pins: each of the 6 finding statuses consumes the projected
 * `finding-status-*` token classes (bg/text/border) — no raw color-mix
 * arbitrary classes.
 */
describe('FindingStatusBadge (finding-status-pill tokens)', () => {
  const cases: Array<[string, string]> = [
    ['open', 'open'],
    ['triaged', 'triaged'],
    ['in_review', 'in-review'],
    ['resolved', 'resolved'],
    ['wont_fix', 'wont-fix'],
    ['duplicate', 'duplicate'],
  ];

  it.each(cases)('status %s consumes finding-status-%s tokens', (status, token) => {
    const { container } = render(<FindingStatusBadge status={status} />);
    const pill = container.querySelector('span') as HTMLElement;
    expect(pill.className).toMatch(new RegExp(`\\bbg-finding-status-${token}-bg\\b`));
    expect(pill.className).toMatch(new RegExp(`\\btext-finding-status-${token}-text\\b`));
    expect(pill.className).toMatch(new RegExp(`\\bborder-finding-status-${token}-border\\b`));
    expect(pill.className).not.toMatch(/color-mix/);
  });

  it('unknown status falls back to the neutral gray chip (no arbitrary values)', () => {
    const { container } = render(<FindingStatusBadge status="something_else" />);
    const pill = container.querySelector('span') as HTMLElement;
    expect(pill.className).toMatch(/\bbg-gray-alpha-100\b/);
    expect(pill.className).not.toMatch(/color-mix/);
  });
});
