/**
 * Profile switcher (plan v1219 T2) — product-behaviour tests:
 *
 *  - Dropdown renders creators from the surface; the active row carries ✓.
 *  - `SOUL {n}%` shows only when the surface supplies a value; silently
 *    omitted otherwise (no fake number, no error state).
 *  - Trailing item is the § Copy string `＋ 新建 Creator…`.
 *  - Switching writes through setActiveCreator, updates the context, and
 *    moves the ✓; a failed switch keeps the previous active creator.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { ProfileSwitcher } from '@/components/shell/profile-switcher';
import { i18n } from '@/lib/i18n/config';
import { BrowserClient } from '@/lib/nexus';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';

const ALICE = { creator_id: 'creator-a', display_name: 'Alice', status: 'active' };
const BOB = { creator_id: 'creator-b', display_name: 'Bob', status: 'active' };

function useCreatorHandlers(putStatus = 200) {
  useHandlers(
    http.get('/v1/daemon/creators', () =>
      HttpResponse.json({
        items: [ALICE, BOB],
        pagination: { limit: 20, has_more: false },
      }),
    ),
    http.put('/v1/daemon/creators/active', () =>
      HttpResponse.json(
        { creator_id: putStatus === 200 ? 'creator-b' : null },
        { status: putStatus },
      ),
    ),
    http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({ agents: [] })),
  );
}

function renderSwitcher() {
  return renderInApp(<ProfileSwitcher />, {
    client: new BrowserClient(),
    activeCreatorId: 'creator-a',
  });
}

async function openMenu(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByTestId('profile-switcher'));
  const menu = await screen.findByTestId('profile-switcher-menu');
  // Wait for the creators query so the rows are rendered before interacting.
  await screen.findAllByRole('menuitem');
  return menu;
}

describe('ProfileSwitcher', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('zh-CN');
  });

  it('lists creators with ✓ on the active row and the § Copy trailing item', async () => {
    useCreatorHandlers();
    const user = userEvent.setup();
    renderSwitcher();
    await openMenu(user);

    const aliceRow = screen.getByRole('menuitem', { name: /Alice/ });
    expect(aliceRow).toHaveTextContent('✓');
    expect(screen.getByRole('menuitem', { name: /Bob/ })).not.toHaveTextContent('✓');
    // The wire contract carries no SOUL maturity → no fake number rendered.
    expect(screen.queryByTestId('soul-maturity')).not.toBeInTheDocument();
    expect(screen.getByTestId('profile-switcher-new')).toHaveTextContent('＋ 新建 Creator…');
  });

  it('renders SOUL {n}% only when the surface supplies a value', async () => {
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [ALICE, { ...BOB, soul_maturity: 62 }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({ agents: [] })),
    );
    const user = userEvent.setup();
    renderSwitcher();
    await openMenu(user);
    expect(screen.getByTestId('soul-maturity')).toHaveTextContent('SOUL 62%');
  });

  it('switches the active creator through setActiveCreator and re-renders', async () => {
    useCreatorHandlers();
    const user = userEvent.setup();
    renderSwitcher();
    await openMenu(user);

    let putBody: unknown = null;
    useHandlers(
      http.put('/v1/daemon/creators/active', async ({ request }) => {
        putBody = await request.json();
        return HttpResponse.json({ creator_id: 'creator-b' });
      }),
    );

    await user.click(screen.getByRole('menuitem', { name: /Bob/ }));

    await waitFor(() => {
      expect(putBody).toEqual({ creator_id: 'creator-b' });
    });
    // Context updated: the switcher button now shows Bob…
    await waitFor(() => {
      expect(screen.getByTestId('profile-switcher')).toHaveTextContent('Bob');
    });
    // …and the ✓ moved to Bob's row on the next open.
    await user.click(screen.getByTestId('profile-switcher'));
    expect(await screen.findByRole('menuitem', { name: /Bob/ })).toHaveTextContent('✓');
  });

  it('keeps the previous creator when the switch fails', async () => {
    useCreatorHandlers(500);
    const user = userEvent.setup();
    renderSwitcher();
    await openMenu(user);
    await user.click(screen.getByRole('menuitem', { name: /Bob/ }));
    await waitFor(() => {
      expect(screen.queryByTestId('profile-switcher-menu')).not.toBeInTheDocument();
    });
    expect(screen.getByTestId('profile-switcher')).toHaveTextContent('Alice');
  });
});
