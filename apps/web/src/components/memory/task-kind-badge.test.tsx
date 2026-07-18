import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';

import { TaskKindBadge } from './task-kind-badge';

/**
 * V1.121 P1 T3 — TaskKindBadge tokenization (DESIGN.md
 * components.memory-task-kind-*).
 *
 * Pins: each known task-kind consumes the projected `memory-task-kind-*`
 * token classes (bg/text/border) — no raw color-mix arbitrary classes.
 */
describe('TaskKindBadge (memory-task-kind tokens)', () => {
  const cases: Array<[string, string]> = [
    ['brainstorm', 'brainstorm'],
    ['outline', 'outline'],
    ['chapter', 'chapter'],
    ['research', 'research'],
    ['unknown', 'unknown'],
  ];

  it.each(cases)('task kind %s consumes memory-task-kind-%s tokens', (kind, token) => {
    const { container } = render(<TaskKindBadge taskKind={kind} />);
    const chip = container.querySelector('span') as HTMLElement;
    expect(chip.className).toMatch(new RegExp(`\\bbg-memory-task-kind-${token}-bg\\b`));
    expect(chip.className).toMatch(new RegExp(`\\btext-memory-task-kind-${token}-text\\b`));
    expect(chip.className).toMatch(new RegExp(`\\bborder-memory-task-kind-${token}-border\\b`));
    expect(chip.className).not.toMatch(/color-mix/);
  });

  it('unrecognized task kind falls back to the unknown chip', () => {
    const { container } = render(<TaskKindBadge taskKind="freeform" />);
    const chip = container.querySelector('span') as HTMLElement;
    expect(chip.className).toMatch(/\bbg-memory-task-kind-unknown-bg\b/);
    expect(chip.className).not.toMatch(/color-mix/);
  });
});
