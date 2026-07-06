import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { FooterProfiles } from './footer-profiles';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';

function makeClient(): BrowserClient {
  return new BrowserClient();
}

function renderFooter(options: Parameters<typeof renderInApp>[1] = {}) {
  return renderInApp(<FooterProfiles />, {
    client: makeClient(),
    activeCreatorId: 'creator-a',
    ...options,
  });
}

describe('FooterProfiles', () => {
  it('renders creator avatars with display-name initials', async () => {
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [
            { creator_id: 'creator-a', display_name: 'Alice' },
            { creator_id: 'creator-b', display_name: 'Bob' },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderFooter();

    expect(await screen.findByTitle('Alice')).toHaveTextContent('A');
    expect(screen.getByTitle('Bob')).toHaveTextContent('B');
  });

  it('marks the active creator avatar as pressed', async () => {
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [
            { creator_id: 'creator-a', display_name: 'Alice' },
            { creator_id: 'creator-b', display_name: 'Bob' },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderFooter({ activeCreatorId: 'creator-b' });

    await waitFor(() => expect(screen.getByTitle('Bob')).toHaveAttribute('aria-pressed', 'true'));
    expect(screen.getByTitle('Alice')).toHaveAttribute('aria-pressed', 'false');
  });

  it('switches the active creator when a non-active avatar is clicked', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [
            { creator_id: 'creator-a', display_name: 'Alice' },
            { creator_id: 'creator-b', display_name: 'Bob' },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderFooter({ activeCreatorId: 'creator-a' });

    await waitFor(() => expect(screen.getByTitle('Bob')).toBeInTheDocument());
    await user.click(screen.getByTitle('Bob'));

    expect(screen.getByTitle('Bob')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTitle('Alice')).toHaveAttribute('aria-pressed', 'false');
  });

  it('opens the create-creator dialog and submits a new profile', async () => {
    const user = userEvent.setup();
    let posted = false;

    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.post('/v1/daemon/creators', async ({ request }) => {
        const body = (await request.json()) as { display_name: string };
        posted = true;
        return HttpResponse.json(
          { creator_id: 'creator-c', display_name: body.display_name },
          { status: 201 },
        );
      }),
    );

    renderFooter();

    await waitFor(() => expect(screen.getByRole('button', { name: 'Add creator' })).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Add creator' }));

    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText('Add Creator')).toBeInTheDocument();

    await user.type(screen.getByLabelText('Display name'), 'Carol');
    await user.click(screen.getByRole('button', { name: /Create$/i }));

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(posted).toBe(true);
  });
});
