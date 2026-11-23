import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes } from 'react-router';

import { FooterProfiles } from './footer-profiles';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient, NexusClientError, type TransportErrorKind } from '@/lib/nexus';
import type { NexusClient } from '@/lib/nexus';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

function makeClient(): BrowserClient {
  return new BrowserClient();
}

function makeDesktop(overrides: Partial<DesktopCapabilities> = {}): DesktopCapabilities {
  return {
    openWith: vi.fn().mockResolvedValue(undefined),
    openExternalUrl: vi.fn().mockResolvedValue(undefined),
    revealInFinder: vi.fn().mockResolvedValue(undefined),
    getDaemonStatus: vi.fn().mockResolvedValue({ state: 'running', port: 8420 }),
    onDaemonStatusChanged: vi.fn().mockResolvedValue(() => {}),
    startDaemon: vi.fn().mockResolvedValue(undefined),
    stopDaemon: vi.fn().mockResolvedValue(undefined),
    resetLocalDatabase: vi.fn().mockResolvedValue({ status: 'confirmed' }),
    getSetupCompleted: vi.fn().mockResolvedValue(true),
    setSetupCompleted: vi.fn().mockResolvedValue(undefined),
    getEntrance: vi.fn().mockResolvedValue('content-creator'),
    setEntrance: vi.fn().mockResolvedValue(undefined),
    setAgentProfile: vi.fn().mockResolvedValue(undefined),
    getAgentProfile: vi.fn().mockResolvedValue(null),
    getWorkspaceRoot: vi.fn().mockResolvedValue('/cached/root'),
    pickDirectory: vi.fn().mockResolvedValue(null),
    setWorkspacePath: vi.fn().mockResolvedValue(undefined),
    switchActiveCreator: vi.fn().mockResolvedValue('/cached/root'),
    restartDaemon: vi.fn().mockResolvedValue(undefined),
    toggleMaximizeWindow: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}

const creatorsHandler = http.get('/v1/daemon/creators', () =>
  HttpResponse.json({
    items: [
      { creator_id: 'creator-a', display_name: 'Alice' },
      { creator_id: 'creator-b', display_name: 'Bob' },
    ],
    pagination: { limit: 20, has_more: false },
  }),
);

function renderFooter(options: Parameters<typeof renderInApp>[1] = {}) {
  return renderInApp(<FooterProfiles />, {
    client: makeClient(),
    activeCreatorId: 'creator-a',
    ...options,
  });
}

describe('FooterProfiles', () => {
  it('renders creator avatars with display-name initials', async () => {
    useHandlers(creatorsHandler);

    renderFooter();

    expect(await screen.findByTitle('Alice')).toHaveTextContent('A');
    expect(screen.getByTitle('Bob')).toHaveTextContent('B');
  });

  it('marks the active creator avatar as pressed', async () => {
    useHandlers(creatorsHandler);

    renderFooter({ activeCreatorId: 'creator-b' });

    await waitFor(() => expect(screen.getByTitle('Bob')).toHaveAttribute('aria-pressed', 'true'));
    expect(screen.getByTitle('Alice')).toHaveAttribute('aria-pressed', 'false');
    expect(screen.getByTitle('Bob')).toHaveClass(
      'bg-footer-profile-avatar-bg-active',
      'text-footer-profile-avatar-text-active',
    );
  });

  it('switches the active creator via switch_active_creator on desktop', async () => {
    const user = userEvent.setup();
    const desktop = makeDesktop();

    useHandlers(creatorsHandler);
    renderFooter({ desktop });

    await waitFor(() => expect(screen.getByTitle('Bob')).toBeInTheDocument());
    await user.click(screen.getByTitle('Bob'));

    await waitFor(() => expect(desktop.switchActiveCreator).toHaveBeenCalledWith('creator-b'));
    expect(desktop.getWorkspaceRoot).toHaveBeenCalled();
    expect(screen.getByTitle('Bob')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTitle('Alice')).toHaveAttribute('aria-pressed', 'false');
    expect(screen.queryByTestId('footer-profile-switch-honesty')).not.toBeInTheDocument();
  });

  it('shows restart-honesty banner when the switched path differs from the cached root', async () => {
    const user = userEvent.setup();
    const desktop = makeDesktop({
      switchActiveCreator: vi.fn().mockResolvedValue('/new/path'),
    });

    useHandlers(creatorsHandler);
    renderFooter({ desktop });

    await waitFor(() => expect(screen.getByTitle('Bob')).toBeInTheDocument());
    await user.click(screen.getByTitle('Bob'));

    await waitFor(() =>
      expect(screen.getByTestId('footer-profile-switch-honesty')).toBeInTheDocument(),
    );
    expect(screen.getByText(/Profile switched/i)).toBeInTheDocument();
  });

  it('surfaces a toast when the desktop switch fails', async () => {
    const user = userEvent.setup();
    const desktop = makeDesktop({
      switchActiveCreator: vi.fn().mockRejectedValue(new Error('config locked')),
    });

    useHandlers(creatorsHandler);
    renderFooter({ desktop });

    await waitFor(() => expect(screen.getByTitle('Bob')).toBeInTheDocument());
    await user.click(screen.getByTitle('Bob'));

    await waitFor(() => expect(screen.getByText('Could not switch Creator')).toBeInTheDocument());
    expect(screen.getByTitle('Alice')).toHaveAttribute('aria-pressed', 'true');
  });

  it('does not switch creator and shows desktop-only honesty in browser mode', async () => {
    const user = userEvent.setup();

    useHandlers(creatorsHandler);
    renderFooter({ desktop: null });

    await waitFor(() => expect(screen.getByTitle('Bob')).toBeInTheDocument());
    await user.click(screen.getByTitle('Bob'));

    expect(screen.getByTitle('Bob')).toHaveAttribute('aria-pressed', 'false');
    expect(screen.getByTitle('Alice')).toHaveAttribute('aria-pressed', 'true');
    expect(
      screen.getByTestId('footer-profile-browser-notice'),
    ).toHaveTextContent(/desktop app only/i);
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

  it('uses roving tabindex so only the focused avatar is in the Tab sequence', async () => {
    useHandlers(creatorsHandler);

    renderFooter();

    const alice = await screen.findByTitle('Alice');
    const bob = screen.getByTitle('Bob');
    const add = screen.getByRole('button', { name: 'Add creator' });

    expect(alice).toHaveAttribute('tabindex', '0');
    expect(bob).toHaveAttribute('tabindex', '-1');
    expect(add).toHaveAttribute('tabindex', '-1');
  });

  it('moves focus with arrow keys inside the toolbar', async () => {
    const user = userEvent.setup();
    useHandlers(creatorsHandler);

    renderFooter();

    const alice = await screen.findByTitle('Alice');
    const bob = screen.getByTitle('Bob');

    await user.click(alice);
    expect(alice).toHaveFocus();

    await user.keyboard('{ArrowRight}');
    expect(bob).toHaveFocus();
    expect(bob).toHaveAttribute('tabindex', '0');
    expect(alice).toHaveAttribute('tabindex', '-1');

    await user.keyboard('{ArrowLeft}');
    expect(alice).toHaveFocus();
    expect(alice).toHaveAttribute('tabindex', '0');
    expect(bob).toHaveAttribute('tabindex', '-1');
  });

  it('jumps to first/last avatar with Home/End', async () => {
    const user = userEvent.setup();
    useHandlers(creatorsHandler);

    renderFooter();

    const alice = await screen.findByTitle('Alice');
    const add = screen.getByRole('button', { name: 'Add creator' });

    add.focus();
    expect(add).toHaveFocus();

    await user.keyboard('{Home}');
    expect(alice).toHaveFocus();

    add.focus();
    await user.keyboard('{End}');
    expect(add).toHaveFocus();
  });

  it('moves focus out of the toolbar on Escape', async () => {
    const user = userEvent.setup();
    useHandlers(creatorsHandler);

    renderFooter();

    const alice = await screen.findByTitle('Alice');
    alice.focus();
    expect(alice).toHaveFocus();

    await user.keyboard('{Escape}');
    expect(alice).not.toHaveFocus();
  });
});

function makeRejectingClient(kind: TransportErrorKind): NexusClient {
  return {
    createCreator: vi.fn().mockRejectedValue(
      new NexusClientError(
        0,
        'transport_unreachable',
        'classifier test fixture',
        { cause: 'fixture' },
        kind,
      ),
    ),
  } as unknown as NexusClient;
}

const creatorsListHandler = http.get('/v1/daemon/creators', () =>
  HttpResponse.json({
    items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
    pagination: { limit: 20, has_more: false },
  }),
);

async function openCreateDialogAndSubmit() {
  const user = userEvent.setup();
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Add creator' })).toBeInTheDocument(),
  );
  await user.click(screen.getByRole('button', { name: 'Add creator' }));
  await user.type(screen.getByLabelText('Display name'), 'Carol');
  await user.click(screen.getByRole('button', { name: /Create$/i }));
}

describe('CreateCreatorDialog classified transport error UX', () => {
  it.each([
    ['daemon_down', /Local daemon is not running/i, /Retry/i, null],
    ['http_fallback', /The app could not complete this request/i, /Retry/i, null],
    ['timeout', /The daemon took too long to respond/i, /Retry/i, /Open Connection Settings/i],
    ['unknown', /Could not reach the daemon/i, /Retry/i, /Open Connection Settings/i],
  ] as Array<[TransportErrorKind, RegExp, RegExp, RegExp | null]>)(
    'renders the classified headline, body, and primary CTA for kind=%s',
    async (kind, headline, primaryCta, secondaryCta) => {
      useHandlers(creatorsListHandler);
      const client = makeRejectingClient(kind);
      renderInApp(<FooterProfiles />, {
        client,
        activeCreatorId: 'creator-a',
        initialRouterEntries: ['/'],
      });

      await openCreateDialogAndSubmit();

      const region = await screen.findByTestId('create-creator-transport-error');
      expect(region).toHaveAttribute('data-kind', kind);
      expect(within(region).getByText(headline)).toBeInTheDocument();
      expect(within(region).getByRole('button', { name: primaryCta })).toBeInTheDocument();
      if (secondaryCta) {
        expect(
          within(region).getByRole('button', { name: secondaryCta }),
        ).toBeInTheDocument();
      } else {
        expect(
          within(region).queryByTestId('create-creator-error-secondary'),
        ).not.toBeInTheDocument();
      }
    },
  );

  it('uses Open Connection Settings as the primary CTA for kind=network', async () => {
    useHandlers(creatorsListHandler);
    const client = makeRejectingClient('network');
    renderInApp(<FooterProfiles />, {
      client,
      activeCreatorId: 'creator-a',
      initialRouterEntries: ['/'],
    });

    await openCreateDialogAndSubmit();

    const region = await screen.findByTestId('create-creator-transport-error');
    expect(region).toHaveAttribute('data-kind', 'network');
    expect(
      within(region).getByRole('button', { name: /Open Connection Settings/i }),
    ).toBeInTheDocument();
    expect(
      within(region).getByRole('button', { name: /Retry/i }),
    ).toBeInTheDocument();
  });

  it('uses Use Desktop App as the primary CTA for kind=tls', async () => {
    useHandlers(creatorsListHandler);
    const client = makeRejectingClient('tls');
    renderInApp(<FooterProfiles />, {
      client,
      activeCreatorId: 'creator-a',
      initialRouterEntries: ['/'],
    });

    await openCreateDialogAndSubmit();

    const region = await screen.findByTestId('create-creator-transport-error');
    expect(region).toHaveAttribute('data-kind', 'tls');
    expect(
      within(region).getByRole('button', { name: /Use Desktop App/i }),
    ).toBeInTheDocument();
    expect(
      within(region).getByRole('button', { name: /Open Connection Settings/i }),
    ).toBeInTheDocument();
  });

  it('re-submits the same display name when Retry is clicked (AC-V1129-P0-3)', async () => {
    useHandlers(creatorsListHandler);
    const client = makeRejectingClient('daemon_down');
    renderInApp(<FooterProfiles />, {
      client,
      activeCreatorId: 'creator-a',
      initialRouterEntries: ['/'],
    });

    await openCreateDialogAndSubmit();
    const region = await screen.findByTestId('create-creator-transport-error');

    await userEvent.setup().click(
      within(region).getByRole('button', { name: /Retry/i }),
    );

    await waitFor(() => {
      expect(client.createCreator).toHaveBeenCalledTimes(2);
    });
    expect(client.createCreator).toHaveBeenNthCalledWith(2, { display_name: 'Carol' });
  });

  it('does not surface the classified region for HTTP errors (no kind)', async () => {
    useHandlers(
      creatorsListHandler,
      http.post('/v1/daemon/creators', () =>
        HttpResponse.json(
          { success: false, error: { code: 'invalid_input', message: 'name too short' } },
          { status: 400 },
        ),
      ),
    );
    renderInApp(<FooterProfiles />, {
      client: new BrowserClient(),
      activeCreatorId: 'creator-a',
      initialRouterEntries: ['/'],
    });

    await openCreateDialogAndSubmit();

    await waitFor(() =>
      expect(screen.queryByTestId('create-creator-transport-error')).not.toBeInTheDocument(),
    );
  });

  function renderWithSettingsTracker(client: NexusClient) {
    return renderInApp(
      <>
        <FooterProfiles />
        <Routes>
          <Route
            path="/settings/advanced"
            element={<div data-testid="nav-settings-advanced" />}
          />
        </Routes>
      </>,
      {
        client,
        activeCreatorId: 'creator-a',
        initialRouterEntries: ['/'],
      },
    );
  }

  it('network primary CTA navigates to /settings/advanced and does NOT retry (QC3-F-001)', async () => {
    const user = userEvent.setup();
    useHandlers(creatorsListHandler);
    const client = makeRejectingClient('network');
    renderWithSettingsTracker(client);

    await openCreateDialogAndSubmit();

    const region = await screen.findByTestId('create-creator-transport-error');
    expect(client.createCreator).toHaveBeenCalledTimes(1);

    await user.click(within(region).getByTestId('create-creator-error-primary'));

    await waitFor(() =>
      expect(screen.getByTestId('nav-settings-advanced')).toBeInTheDocument(),
    );
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(client.createCreator).toHaveBeenCalledTimes(1);
  });

  it('tls primary CTA is informational — no retry, no navigation (QC3-F-001)', async () => {
    const user = userEvent.setup();
    useHandlers(creatorsListHandler);
    const client = makeRejectingClient('tls');
    renderWithSettingsTracker(client);

    await openCreateDialogAndSubmit();

    const region = await screen.findByTestId('create-creator-transport-error');
    expect(client.createCreator).toHaveBeenCalledTimes(1);

    await user.click(within(region).getByTestId('create-creator-error-primary'));

    expect(client.createCreator).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId('nav-settings-advanced')).not.toBeInTheDocument();
    expect(screen.getByTestId('create-creator-transport-error')).toBeInTheDocument();
  });

  it('regression guard: retry-default kinds still re-submit via the primary CTA (QC3-F-001)', async () => {
    const user = userEvent.setup();
    useHandlers(creatorsListHandler);
    const client = makeRejectingClient('daemon_down');
    renderWithSettingsTracker(client);

    await openCreateDialogAndSubmit();

    const region = await screen.findByTestId('create-creator-transport-error');
    expect(client.createCreator).toHaveBeenCalledTimes(1);

    await user.click(within(region).getByTestId('create-creator-error-primary'));

    await waitFor(() => expect(client.createCreator).toHaveBeenCalledTimes(2));
    expect(client.createCreator).toHaveBeenNthCalledWith(2, { display_name: 'Carol' });
    expect(screen.queryByTestId('nav-settings-advanced')).not.toBeInTheDocument();
  });
});
