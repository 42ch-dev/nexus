import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { SetupCompletedProvider, useSetupCompleted } from './setup-completed-context';
import { renderInApp } from '@/test/test-providers';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import { BrowserClient } from '@/lib/nexus';

function TestController() {
  const { completed, isLoading, markCompleted } = useSetupCompleted();
  return (
    <div>
      <span data-testid="completed">{completed ? 'true' : 'false'}</span>
      <span data-testid="loading">{isLoading ? 'true' : 'false'}</span>
      <button onClick={() => markCompleted()}>Finish</button>
    </div>
  );
}

function makeClient() {
  return new BrowserClient();
}

function makeDesktop(overrides: Partial<DesktopCapabilities> = {}): DesktopCapabilities {
  return {
    openWith: () => Promise.resolve(),
    revealInFinder: () => Promise.resolve(),
    getDaemonStatus: () => Promise.resolve({ state: 'running', port: 8420 }),
    onDaemonStatusChanged: () => Promise.resolve(() => {}),
    startDaemon: () => Promise.resolve(),
    stopDaemon: () => Promise.resolve(),
    getSetupCompleted: () => Promise.resolve(false),
    setSetupCompleted: () => Promise.resolve(),
    setAgentProfile: () => Promise.resolve(),
    getWorkspaceRoot: () => Promise.resolve('/tmp/nexus'),
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

  it('markCompleted persists through the desktop shell', async () => {
    const user = userEvent.setup();
    const setSetupCompleted = vi.fn(() => Promise.resolve());

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

    await waitFor(() => expect(screen.getByTestId('completed')).toHaveTextContent('true'));
    expect(setSetupCompleted).toHaveBeenCalledWith(true);
  });
});
