import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';

import { MaturationIndicators } from './maturation-indicators';

/**
 * V1.121 P1 T3 — MaturationIndicators count-badge tokenization (DESIGN.md
 * components.reading-maturation-badge).
 *
 * Pins: KB-density and open-findings count badges consume the projected
 * `reading-maturation-*` token classes — no raw color-mix arbitrary values.
 */

vi.mock('@/components/reading/reading-hooks', () => ({
  useOpenFindingsCount: vi.fn(),
  useWorldKbDensity: vi.fn(),
}));

const hooks = await import('@/components/reading/reading-hooks');
const mockFindings = vi.mocked(hooks.useOpenFindingsCount);
const mockKb = vi.mocked(hooks.useWorldKbDensity);

function setup(findingsCount: number) {
  mockFindings.mockReturnValue({ count: findingsCount, isLoading: false, truncated: false });
  mockKb.mockReturnValue({ count: 12, isLoading: false });
}

describe('MaturationIndicators (reading-maturation-badge tokens)', () => {
  it('KB density badge consumes reading-maturation-kb-density tokens', () => {
    setup(0);
    render(<MaturationIndicators workId="w1" chapter={1} status="draft" />);
    const kb = screen.getByLabelText(/knowledge entries/i);
    expect(kb.className).toMatch(/\bbg-reading-maturation-kb-density-bg\b/);
    expect(kb.className).toMatch(/\btext-reading-maturation-kb-density-text\b/);
    expect(kb.className).toMatch(/\bborder-reading-maturation-kb-density-border\b/);
    expect(kb.className).not.toMatch(/color-mix/);
  });

  it('open-findings badge consumes open-findings tokens when count > 0', () => {
    setup(3);
    render(<MaturationIndicators workId="w1" chapter={1} status="draft" />);
    const findings = screen.getByLabelText(/open findings/i);
    expect(findings.className).toMatch(/\bbg-reading-maturation-open-findings-bg\b/);
    expect(findings.className).toMatch(/\btext-reading-maturation-open-findings-text\b/);
    expect(findings.className).toMatch(/\bborder-reading-maturation-open-findings-border\b/);
    expect(findings.className).not.toMatch(/color-mix/);
  });

  it('zero open findings uses the neutral gray chip (no arbitrary values)', () => {
    setup(0);
    render(<MaturationIndicators workId="w1" chapter={1} status="draft" />);
    const findings = screen.getByLabelText(/open findings/i);
    expect(findings.className).toMatch(/\bbg-gray-alpha-100\b/);
    expect(findings.className).not.toMatch(/color-mix/);
  });
});
