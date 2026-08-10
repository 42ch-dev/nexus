import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router';

import {
  SettingsModalProvider,
  useSettingsModal,
} from './settings-modal-context';

function Probe() {
  const {
    open,
    activeSection,
    openSettings,
    requestClose,
    registerDirtySource,
    discardConfirmOpen,
  } = useSettingsModal();
  const location = useLocation();
  return (
    <div>
      <span data-testid="open">{String(open)}</span>
      <span data-testid="section">{activeSection}</span>
      <span data-testid="path">{location.pathname}</span>
      <span data-testid="discard">{String(discardConfirmOpen)}</span>
      <button type="button" onClick={() => openSettings('workspace')}>
        Open
      </button>
      <button type="button" onClick={() => requestClose('button')}>
        Close
      </button>
      <button type="button" onClick={() => registerDirtySource('probe', true)}>
        Dirty
      </button>
      <button type="button" onClick={() => registerDirtySource('probe', false)}>
        Clean
      </button>
    </div>
  );
}

function renderProbe(initialEntry = '/works') {
  return render(
    <MemoryRouter initialEntries={[initialEntry]}>
      <SettingsModalProvider>
        <Routes>
          <Route path="*" element={<Probe />} />
        </Routes>
      </SettingsModalProvider>
    </MemoryRouter>,
  );
}

describe('SettingsModalProvider', () => {
  it('exposes openSettings with a stable default section contract', async () => {
    const user = userEvent.setup();
    renderProbe();

    expect(screen.getByTestId('open')).toHaveTextContent('false');
    expect(screen.getByTestId('section')).toHaveTextContent('agent');

    await user.click(screen.getByRole('button', { name: 'Open' }));

    expect(screen.getByTestId('open')).toHaveTextContent('true');
    expect(screen.getByTestId('section')).toHaveTextContent('workspace');
    expect(screen.getByTestId('path')).toHaveTextContent('/settings/workspace');

    await user.click(screen.getByRole('button', { name: 'Close' }));
    expect(screen.getByTestId('open')).toHaveTextContent('false');
    expect(screen.getByTestId('path')).toHaveTextContent('/works');
  });

  it('guards dirty close and restores the background route on confirm', async () => {
    const user = userEvent.setup();
    renderProbe('/sessions');

    await user.click(screen.getByRole('button', { name: 'Open' }));
    await user.click(screen.getByRole('button', { name: 'Dirty' }));
    await user.click(screen.getByRole('button', { name: 'Close' }));

    expect(screen.getByTestId('discard')).toHaveTextContent('true');
    expect(screen.getByTestId('open')).toHaveTextContent('true');

    await user.click(screen.getByRole('button', { name: 'Clean' }));
    await user.click(screen.getByRole('button', { name: 'Close' }));

    expect(screen.getByTestId('open')).toHaveTextContent('false');
    expect(screen.getByTestId('path')).toHaveTextContent('/sessions');
  });
});
