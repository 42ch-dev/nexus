import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { SetupCompletedProvider, useSetupCompleted } from './setup-completed-context';
import { renderInApp } from '@/test/test-providers';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import { BrowserClient } from '@/lib/nexus';

function TestController() {
  const { completed, isLoading, markCompleted, setCompleted } = useSetupCompleted();
  return (
    <div>
      <span data-testid="completed">{completed ? 'true' : 'false'}</span>
      <span data-testid="loading">{isLoading ? 'true' : 'false'}</span>
      <button onClick={() => markCompleted()}>Finish</button>
      <button onClick={() => void setCompleted(false)}>Clear</button>
      <button
        onClick={() => {
          // Await path so IPC rejection is handled (rollback assertion).
          void setCompleted(true).catch(() => undefined);
        }}
      >
        FinishTracked
      </button>
    </div>
  );
}

function makeClient() {
  return new BrowserClient();
}

function makeDesktop(overrides: Partial<DesktopCapabilities> = {}): DesktopCapabilities {
  return {
    openWith: () => Promise.resolve(),
    openExternalUrl: () => Promise.resolve(),
    revealInFinder: () => Promise.resolve(),
    getDaemonStatus: () => Promise.resolve({ state: 'running', port: 8420 }),
    onDaemonStatusChanged: () => Promise.resolve(() => {}),
    startDaemon: () => Promise.resolve(),
    stopDaemon: () => Promise.resolve(),
    resetLocalDatabase: () => Promise.resolve(),
    getSetupCompleted: () => Promise.resolve(false),
    setSetupCompleted: () => Promise.resolve(),
    getEntrance: () => Promise.resolve('content-creator'),
    setEntrance: () => Promise.resolve(),
    setAgentProfile: () => Promise.resolve(),
    getAgentProfile: () => Promise.resolve(null),
    getWorkspaceRoot: () => Promise.resolve('/tmp/nexus'),
    pickDirectory: () => Promise.resolve(null),
    setWorkspacePath: () => Promise.resolve(),
    ensureSetupBootstrap: () =>
      Promise.resolve({ creator_id: 'ctr_local1234567890ab', already_bootstrapped: false }),
    switchActiveCreator: () => Promise.resolve('/tmp/nexus'),
    restartDaemon: () => Promise.resolve(),
    toggleMaximizeWindow: () => Promise.resolve(),
    ...overrides,
  };
}

describe('SetupCompletedProvider', () => {
  it('browser build is immediately completed with no loading', () => {
    renderInApp(
      <SetupCompletedProvider>
        <TestController />
      </SetupCompletedProvider>,
      { client: makeClient() },
    );

    expect(screen.getByTestId('completed')).toHaveTextContent('true');
    expect(screen.getByTestId('loading')).toHaveTextContent('false');
  });

  it('desktop build reads setup state from the shell', async () => {
    renderInApp(
      <SetupCompletedProvider>
        <TestController />
      </SetupCompletedProvider>,
      {
        client: makeClient(),
        desktop: makeDesktop({ getSetupCompleted: () => Promise.resolve(false) }),
      },
    );

    expect(screen.getByTestId('loading')).toHaveTextContent('true');
    await waitFor(() => expect(screen.getByTestId('completed')).toHaveTextContent('false'));
    await waitFor(() => expect(screen.getByTestId('loading')).toHaveTextContent('false'));
  });

  it('markCompleted sets completed true before IPC resolves (SetupGate-safe)', async () => {
    const user = userEvent.setup();
    let resolveIpc!: () => void;
    const setSetupCompleted = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveIpc = resolve;
        }),
    );

    renderInApp(
      <SetupCompletedProvider>
        <TestController />
      </SetupCompletedProvider>,
      {
        client: makeClient(),
        desktop: makeDesktop({ setSetupCompleted }),
      },
    );

    await waitFor(() => expect(screen.getByTestId('loading')).toHaveTextContent('false'));
    await user.click(screen.getByRole('button', { name: 'Finish' }));

    // Optimistic: React state flips before IPC settles so gated navigate is safe.
    expect(screen.getByTestId('completed')).toHaveTextContent('true');
    expect(setSetupCompleted).toHaveBeenCalledWith(true);
    resolveIpc();
    await waitFor(() => expect(screen.getByTestId('completed')).toHaveTextContent('true'));
  });

  it('setCompleted(false) awaits IPC before clearing React state', async () => {
    const user = userEvent.setup();
    let resolveIpc!: () => void;
    const setSetupCompleted = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveIpc = resolve;
        }),
    );

    renderInApp(
      <SetupCompletedProvider initialCompleted>
        <TestController />
      </SetupCompletedProvider>,
      {
        client: makeClient(),
        desktop: makeDesktop({ setSetupCompleted }),
      },
    );

    expect(screen.getByTestId('completed')).toHaveTextContent('true');
    await user.click(screen.getByRole('button', { name: 'Clear' }));

    // R1: stay true until IPC succeeds — avoids stale true after clear.
    expect(setSetupCompleted).toHaveBeenCalledWith(false);
    expect(screen.getByTestId('completed')).toHaveTextContent('true');
    resolveIpc();
    await waitFor(() => expect(screen.getByTestId('completed')).toHaveTextContent('false'));
  });

  it('setCompleted(true) rolls back React state when IPC fails', async () => {
    const user = userEvent.setup();
    const setSetupCompleted = vi.fn(() => Promise.reject(new Error('disk full')));

    renderInApp(
      <SetupCompletedProvider>
        <TestController />
      </SetupCompletedProvider>,
      {
        client: makeClient(),
        desktop: makeDesktop({ setSetupCompleted }),
      },
    );

    await waitFor(() => expect(screen.getByTestId('loading')).toHaveTextContent('false'));
    await user.click(screen.getByRole('button', { name: 'FinishTracked' }));

    await waitFor(() => expect(screen.getByTestId('completed')).toHaveTextContent('false'));
    expect(setSetupCompleted).toHaveBeenCalledWith(true);
  });
});
