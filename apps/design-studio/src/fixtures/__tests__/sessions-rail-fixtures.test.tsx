import { cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { SessionsRailFixtures } from '@/fixtures/sessions-rail-fixtures';

/**
 * The `/sessions` rail has two states worth eyeballing: the mixed
 * running/completed/failed card grid (each card one link to its run view) and
 * the zero-session empty state. Assertions pin what distinguishes them; the
 * theme pass proves the token-backed frame survives the dark toggle.
 */
const VARIANTS = ['sessions-rail-fixture-mixed', 'sessions-rail-fixture-empty'] as const;

afterEach(() => {
  cleanup();
  document.documentElement.classList.remove('dark');
});

describe('SessionsRailFixtures', () => {
  it('mounts every variant frame', () => {
    render(<SessionsRailFixtures />);

    expect(screen.getByTestId('sessions-rail-fixtures')).toBeInTheDocument();
    for (const testId of VARIANTS) {
      expect(screen.getByTestId(testId)).toBeInTheDocument();
    }
  });

  it('renders one whole-card link per session in list order', () => {
    render(<SessionsRailFixtures />);
    const frame = screen.getByTestId('sessions-rail-fixture-mixed');
    const links = within(frame).getAllByRole('link');

    expect(links.map((link) => link.getAttribute('href'))).toEqual([
      '/sessions/sess-7f3a91c2d4e8',
      '/sessions/sess-2b8c4a',
      '/sessions/sess-9d1e0f',
    ]);
    expect(links.map((link) => link.getAttribute('aria-label'))).toEqual([
      'Open run view for session sess-7f3…d4e8',
      'Open run view for session sess-2b8c4a',
      'Open run view for session sess-9d1e0f',
    ]);
    // One affordance per card, nothing interactive nested inside it.
    for (const link of links) {
      expect(within(link).queryAllByRole('link')).toHaveLength(0);
      expect(within(link).queryAllByRole('button')).toHaveLength(0);
    }
  });

  it('shows short id, pill, preset and creator on each card', () => {
    render(<SessionsRailFixtures />);
    const frame = screen.getByTestId('sessions-rail-fixture-mixed');

    const running = within(frame).getByRole('link', {
      name: 'Open run view for session sess-7f3…d4e8',
    });
    expect(within(running).getByText('sess-7f3…d4e8')).toBeInTheDocument();
    expect(within(running).getByText('Preset')).toBeInTheDocument();
    expect(within(running).getByText('novel-writing')).toBeInTheDocument();
    expect(within(running).getByText('Creator')).toBeInTheDocument();
    expect(within(running).getByText('author-01')).toBeInTheDocument();
    // No `current_task_id` on the card.
    expect(within(running).queryByText(/Current task/i)).not.toBeInTheDocument();
  });

  it('paints completed as the green pill and failed as the red one', () => {
    render(<SessionsRailFixtures />);

    expect(
      screen.getByText('Completed').className,
    ).toContain('nexus-ui-badge-soft-running-bg');
    expect(screen.getByText('Failed').className).toContain('nexus-ui-badge-soft-error-bg');
  });

  it('renders the zero-session empty copy with no cards', () => {
    render(<SessionsRailFixtures />);
    const frame = screen.getByTestId('sessions-rail-fixture-empty');

    expect(within(frame).getByText('No sessions yet')).toBeInTheDocument();
    expect(
      within(frame).getByText('Sessions appear here when a preset run starts.'),
    ).toBeInTheDocument();
    expect(within(frame).queryAllByRole('link')).toHaveLength(0);
  });

  it('renders every variant under both themes', () => {
    for (const theme of ['light', 'dark'] as const) {
      document.documentElement.classList.toggle('dark', theme === 'dark');

      render(<SessionsRailFixtures />);

      for (const testId of VARIANTS) {
        expect(screen.getByTestId(testId)).toBeInTheDocument();
      }
      // Token-backed frame class survives the toggle — the card has no theme prop.
      expect(screen.getByTestId('hub-card-session-sess-2b8c4a')).toHaveClass('bg-background-100');

      cleanup();
    }
  });
});
