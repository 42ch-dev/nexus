import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import {
  SettingsModalProvider,
  useSettingsModal,
} from './settings-modal-context';

function Probe() {
  const { open, activeSection, openSettings, closeSettings } = useSettingsModal();
  return (
    <div>
      <span data-testid="open">{String(open)}</span>
      <span data-testid="section">{activeSection}</span>
      <button type="button" onClick={() => openSettings('workspace')}>
        Open
      </button>
      <button type="button" onClick={() => closeSettings()}>
        Close
      </button>
    </div>
  );
}

describe('SettingsModalProvider', () => {
  it('exposes openSettings with a stable default section contract', async () => {
    const user = userEvent.setup();
    render(
      <SettingsModalProvider>
        <Probe />
      </SettingsModalProvider>,
    );

    expect(screen.getByTestId('open')).toHaveTextContent('false');
    expect(screen.getByTestId('section')).toHaveTextContent('agent');

    await user.click(screen.getByRole('button', { name: 'Open' }));

    expect(screen.getByTestId('open')).toHaveTextContent('true');
    expect(screen.getByTestId('section')).toHaveTextContent('workspace');

    await user.click(screen.getByRole('button', { name: 'Close' }));
    expect(screen.getByTestId('open')).toHaveTextContent('false');
  });
});
