/**
 * SoulNarrativeCard tests (V1.81 SP-1 — web-ui.md §26.1).
 *
 * The card is stateless beyond its props, so each of the five UX states is
 * exercised by rendering with a representative prop combination and asserting
 * the contract copy + CTA per plan §2.1:
 *  - ungenerated (CTA), generating (loading), current (cached + timestamp),
 *    stale (banner + cached), insufficient-data (encouraging empty + count).
 */
import { screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { SoulNarrativeCard } from '@/components/soul/soul-narrative-card';
import { renderInApp } from '@/test/test-providers';
import type { SoulNarrativeResponse } from '@42ch/nexus-contracts';

function response(over: Partial<SoulNarrativeResponse>): SoulNarrativeResponse {
  return {
    creator_id: 'c1',
    state: 'current',
    narrative: undefined,
    generated_at: undefined,
    stale: false,
    current_fragment_count: 0,
    current_distinct_keyword_count: 0,
    min_fragment_count: 10,
    min_distinct_keyword_count: 20,
    ...over,
  };
}

describe('SoulNarrativeCard — five UX states', () => {
  it('ungenerated: shows the Reflect CTA with the preview hint', () => {
    const onReflect = vi.fn();
    renderInApp(
      <SoulNarrativeCard
        narrative={response({ state: 'ungenerated' })}
        isLoading={false}
        isReflecting={false}
        onReflect={onReflect}
      />,
    );
    expect(screen.getByTestId('soul-narrative-ungenerated')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /reflect on my soul/i })).toBeInTheDocument();
    expect(screen.getByText(/nexus will reflect on your themes/i)).toBeInTheDocument();
  });

  it('generating: shows the reflecting spinner + disabled button', () => {
    renderInApp(
      <SoulNarrativeCard
        narrative={response({ state: 'ungenerated' })}
        isLoading={false}
        isReflecting
        onReflect={() => {}}
      />,
    );
    expect(screen.getByTestId('soul-narrative-generating')).toBeInTheDocument();
    const button = screen.getByTestId('soul-narrative-reflect');
    expect(button).toBeDisabled();
    // "Reflecting…" appears in both the status line and the disabled button label.
    expect(screen.getAllByText(/reflecting…/i).length).toBeGreaterThanOrEqual(1);
  });

  it('current: shows cached prose + generated_at timestamp + Re-reflect', () => {
    renderInApp(
      <SoulNarrativeCard
        narrative={response({
          state: 'current',
          narrative: 'You are drawn to moral ambiguity and ensemble casts.',
          generated_at: '2026-07-01T12:00:00Z',
          current_fragment_count: 42,
        })}
        isLoading={false}
        isReflecting={false}
        onReflect={() => {}}
      />,
    );
    expect(screen.getByTestId('soul-narrative-current')).toBeInTheDocument();
    expect(screen.getByTestId('soul-narrative-prose')).toHaveTextContent(
      'You are drawn to moral ambiguity and ensemble casts.',
    );
    // No stale banner in the current state.
    expect(screen.queryByTestId('soul-narrative-stale-banner')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: /re-reflect/i })).toBeInTheDocument();
    expect(screen.getByText(/reflected on/i)).toBeInTheDocument();
  });

  it('stale: shows the growth banner above the cached prose with a Re-reflect CTA', () => {
    renderInApp(
      <SoulNarrativeCard
        narrative={response({
          state: 'stale',
          stale: true,
          narrative: 'An earlier reflection on your themes.',
          generated_at: '2026-06-01T12:00:00Z',
        })}
        isLoading={false}
        isReflecting={false}
        onReflect={() => {}}
      />,
    );
    const banner = screen.getByTestId('soul-narrative-stale-banner');
    expect(banner).toBeInTheDocument();
    expect(banner).toHaveTextContent(/you've grown since this reflection/i);
    // The cached narrative stays visible under the banner.
    expect(screen.getByTestId('soul-narrative-prose')).toHaveTextContent(
      'An earlier reflection on your themes.',
    );
    // The banner carries its own Re-reflect CTA.
    expect(
      screen.getAllByRole('button', { name: /re-reflect/i }).length,
    ).toBeGreaterThanOrEqual(1);
  });

  it('insufficient-data: shows the encouraging empty state + fragment count', () => {
    renderInApp(
      <SoulNarrativeCard
        narrative={response({
          state: 'insufficient_data',
          current_fragment_count: 3,
          min_fragment_count: 10,
        })}
        isLoading={false}
        isReflecting={false}
        onReflect={() => {}}
      />,
    );
    expect(screen.getByTestId('soul-narrative-insufficient')).toBeInTheDocument();
    expect(screen.getByText(/your soul is still forming/i)).toBeInTheDocument();
    expect(screen.getByText(/3 fragments captured so far/i)).toBeInTheDocument();
    expect(screen.getByText(/7 more to go/i)).toBeInTheDocument();
    // No CTA in the insufficient-data state.
    expect(screen.queryByTestId('soul-narrative-reflect')).not.toBeInTheDocument();
  });

  it('insufficient-data: omits the "more to go" hint when already at/over the threshold', () => {
    renderInApp(
      <SoulNarrativeCard
        narrative={response({
          state: 'insufficient_data',
          current_fragment_count: 12,
          min_fragment_count: 10,
        })}
        isLoading={false}
        isReflecting={false}
        onReflect={() => {}}
      />,
    );
    expect(screen.getByText(/12 fragments captured so far/i)).toBeInTheDocument();
    expect(screen.queryByText(/more to go/i)).not.toBeInTheDocument();
  });
});
