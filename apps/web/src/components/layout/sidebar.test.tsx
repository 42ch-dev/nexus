import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { Sidebar } from './sidebar';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';

vi.mock('@/components/brand/nexus-logo', () => ({
  NexusLogo: () => <div data-testid="nexus-logo">Nexus</div>,
}));

function makeClient() {
  return new BrowserClient();
}

describe('Sidebar', () => {
  it('renders the Creator tab by default', async () => {
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByRole('tab', { name: 'Creator', selected: true })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'All Works' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Memory' })).toBeInTheDocument();
  });

  it('swaps to Orchestrator tab and shows runtime/strategy links', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));

    expect(screen.getByRole('tab', { name: 'Orchestrator', selected: true })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Sessions' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Schedule' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Capabilities' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Strategies' })).toBeInTheDocument();

    expect(screen.queryByRole('link', { name: 'All Works' })).not.toBeInTheDocument();
  });

  it('does not expose Connect or Daemon as top-level nav items', async () => {
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.queryByRole('link', { name: /Connect/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: /Daemon/i })).not.toBeInTheDocument();
  });

  it('wraps tabs in a tablist and exposes the nav groups as a tabpanel', async () => {
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByRole('tablist', { name: 'Primary navigation' })).toBeInTheDocument();
    expect(screen.getByRole('tabpanel')).toHaveAttribute('aria-labelledby', 'creator');
  });

  it('mounts the footer profile switcher', async () => {
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await waitFor(() =>
      expect(screen.getByRole('toolbar', { name: 'Profiles' })).toBeInTheDocument(),
    );
  });

  it('exposes Settings as a footer utility link above profiles', async () => {
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    const link = screen.getByTestId('settings-footer-utility-link');
    expect(link).toHaveAttribute('href', '/settings');
    expect(link).toHaveTextContent('Settings');
    // Settings stays visible on Creator tab (not tab-scoped).
    expect(screen.getByRole('tab', { name: 'Creator', selected: true })).toBeInTheDocument();
  });
});
