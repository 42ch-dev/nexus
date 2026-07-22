import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes } from 'react-router-dom';

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
    resetLocalDatabase: vi.fn().mockResolvedValue(undefined),
    getSetupCompleted: vi.fn().mockResolvedValue(true),
    setSetupCompleted: vi.fn().mockResolvedValue(undefined),
    setAgentProfile: vi.fn().mockResolvedValue(undefined),
    getAgentProfile: vi.fn().mockResolvedValue(null),
    getWorkspaceRoot: vi.fn().mockResolvedValue('/cached/root'),
    pickDirectory: vi.fn().mockResolvedValue(null),
    setWorkspacePath: vi.fn().mockResolvedValue(undefined),
    ensureSetupBootstrap: vi.fn().mockResolvedValue({
      creator_id: 'creator-a',
      already_bootstrapped: true,
    }),
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

// ── CreateCreatorDialog classified error region (V1.129 P0 T4) ──────────────
//
// The dialog consumes `NexusClientError.kind` and renders the matching
// headline + body + CTA from the spec table. We drive each kind by injecting a
// stub NexusClient whose `createCreator` rejects with a pre-classified error,
// so the dialog UX is tested independently of the classifier (covered in
// browser-client.test.ts). See `.mstar/iterations/v1.129/specs/profile-create-reliability.md`
// § Dialog UX contract for the locked copy + CTA table.

/**
 * Build a NexusClient stub whose `createCreator` rejects with a NexusClientError
 * carrying the given kind. Other NexusClient methods stay no-op rejects so the
 * dialog renders without a real transport.
 */
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
    // Secondary CTA is Retry for network per spec table.
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

    // Click Retry; expect createCreator to be invoked a second time with the
    // same payload. The transport will still reject (the stub always rejects),
    // so the dialog stays open and the region remains visible.
    await userEvent.setup().click(
      within(region).getByRole('button', { name: /Retry/i }),
    );

    await waitFor(() => {
      expect(client.createCreator).toHaveBeenCalledTimes(2);
    });
    expect(client.createCreator).toHaveBeenNthCalledWith(2, { display_name: 'Carol' });
  });

  it('does not surface the classified region for HTTP errors (no kind)', async () => {
    // HTTP errors carry a status + code (fromBody) but no `kind` — they are
    // handled by the global toast via useCreateCreator.onError, not the
    // dialog's transport-recovery region.
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

    // Wait a tick for the mutation to settle; the classified region must never mount.
    await waitFor(() =>
      expect(screen.queryByTestId('create-creator-transport-error')).not.toBeInTheDocument(),
    );
  });

  // ── QC3-F-001 regression: primary CTA onClick must branch per kind ──────────
  //
  // The primary CTA's label varies per `primaryCtaForKind`, so its click handler
  // must vary the same way: `openConnectionSettings` navigates, `useDesktopApp`
  // is informational (no-op), `retry` re-submits. These tests pin the contract
  // for each branch so a future regression that re-hardcodes `handleRetry` is
  // caught immediately.

  /**
   * Render FooterProfiles alongside a Routes tracker that surfaces a marker when
   * the router lands on `/settings/advanced`. The dialog's
   * `handleOpenConnectionSettings` calls `navigate('/settings/advanced#connection')`,
   * which this marker verifies.
   */
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
    // The initial form submit already triggered one createCreator call.
    expect(client.createCreator).toHaveBeenCalledTimes(1);

    // Click the PRIMARY CTA by testid so the assertion is surgical — it
    // targets the button whose onClick the QC3-F-001 fix branched, regardless
    // of label collisions with the secondary CTA on other kinds.
    await user.click(within(region).getByTestId('create-creator-error-primary'));

    // Navigation must have happened.
    await waitFor(() =>
      expect(screen.getByTestId('nav-settings-advanced')).toBeInTheDocument(),
    );
    // The dialog closes as part of handleOpenConnectionSettings.
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    // Regression guard: createCreator was NOT invoked a second time.
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

    // Click the primary CTA ("Use Desktop App") — should be a true no-op.
    await user.click(within(region).getByTestId('create-creator-error-primary'));

    // No second mutate call.
    expect(client.createCreator).toHaveBeenCalledTimes(1);
    // No navigation — the settings marker must not appear.
    expect(screen.queryByTestId('nav-settings-advanced')).not.toBeInTheDocument();
    // The dialog stays open (onOpenChange was never called).
    expect(screen.getByTestId('create-creator-transport-error')).toBeInTheDocument();
  });

  it('regression guard: retry-default kinds still re-submit via the primary CTA (QC3-F-001)', async () => {
    // For `daemon_down`, primaryCtaForKind returns 'retry'. The primary CTA's
    // onClick must still call mutate (regression guard against accidentally
    // breaking the retry branch while adding the openConnectionSettings /
    // useDesktopApp branches).
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
    // No navigation — retry keeps the user in the dialog.
    expect(screen.queryByTestId('nav-settings-advanced')).not.toBeInTheDocument();
  });
});
