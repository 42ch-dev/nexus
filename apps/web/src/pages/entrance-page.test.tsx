/**
 * Entrance identity page (V1.170 P1 — AR-16/AR-20, product EL §2).
 *
 * Pins the locked copy, the default content-creator highlight, the `?entrance=`
 * pre-highlight (session-only), and the persist-ONLY-on-Continue contract:
 * selecting an option writes nothing; Continue persists and lands on the
 * chosen entrance's land route.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes, useLocation } from 'react-router';

import { EntrancePage } from '@/pages/entrance-page';
import { EntranceProvider, ENTRANCE_STORAGE_KEY } from '@/lib/entrance-context';
import { renderInApp } from '@/test/test-providers';

function LocationDisplay() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

function renderPage(initialRouterEntries: string[]) {
  return renderInApp(
    <>
      <LocationDisplay />
      <EntranceProvider>
        <Routes>
          <Route path="entrance" element={<EntrancePage />} />
          <Route path="works" element={<div data-testid="works-route">Works</div>} />
          <Route path="developer" element={<div data-testid="developer-route">Developer</div>} />
        </Routes>
      </EntranceProvider>
    </>,
    { initialRouterEntries },
  );
}

describe('EntrancePage', () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it('renders the locked EL §2 copy with both options and the Continue CTA', () => {
    renderPage(['/entrance']);

    expect(
      screen.getByRole('heading', { name: 'How do you use Nexus?' }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        'This chooses your workspace layout. It is not your writing identity — Creator profiles stay as they are.',
      ),
    ).toBeInTheDocument();

    const contentCreator = screen.getByTestId('entrance-option-content-creator');
    expect(contentCreator).toHaveTextContent('Content creator');
    expect(contentCreator).toHaveTextContent(
      'Write and worldbuild. Agents help you. You do not install modules or edit presets.',
    );

    const developer = screen.getByTestId('entrance-option-developer');
    expect(developer).toHaveTextContent('Developer');
    expect(developer).toHaveTextContent(
      'Build on Nexus. Modules, presets, capabilities, Connect, and the full Control Room.',
    );

    expect(screen.getByRole('button', { name: 'Continue' })).toBeInTheDocument();
  });

  it('defaults the highlight to content-creator', () => {
    renderPage(['/entrance']);
    expect(screen.getByTestId('entrance-option-content-creator')).toHaveAttribute(
      'aria-checked',
      'true',
    );
    expect(screen.getByTestId('entrance-option-developer')).toHaveAttribute(
      'aria-checked',
      'false',
    );
  });

  it('pre-highlights the ?entrance= override (session-only, AR-20)', () => {
    renderPage(['/entrance?entrance=developer']);
    expect(screen.getByTestId('entrance-option-developer')).toHaveAttribute(
      'aria-checked',
      'true',
    );
    expect(screen.getByTestId('entrance-option-content-creator')).toHaveAttribute(
      'aria-checked',
      'false',
    );
  });

  it('does NOT persist on selection alone — only Continue writes (AR-20)', async () => {
    const user = userEvent.setup();
    renderPage(['/entrance']);
    await user.click(screen.getByTestId('entrance-option-developer'));
    expect(window.localStorage.getItem(ENTRANCE_STORAGE_KEY)).toBeNull();
  });

  it('Continue persists the selection and lands on the chosen land route', async () => {
    const user = userEvent.setup();
    renderPage(['/entrance']);
    await user.click(screen.getByTestId('entrance-option-developer'));
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(screen.getByTestId('developer-route')).toBeInTheDocument());
    expect(screen.getByTestId('location')).toHaveTextContent('/developer');
    expect(window.localStorage.getItem(ENTRANCE_STORAGE_KEY)).toBe('developer');
  });

  it('Continue with the default selection persists content-creator and lands on /works', async () => {
    const user = userEvent.setup();
    renderPage(['/entrance']);
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(screen.getByTestId('works-route')).toBeInTheDocument());
    expect(screen.getByTestId('location')).toHaveTextContent('/works');
    expect(window.localStorage.getItem(ENTRANCE_STORAGE_KEY)).toBe('content-creator');
  });

  it('shows an error toast and stays on the page when persistence fails', async () => {
    const user = userEvent.setup();
    const originalSetItem = Storage.prototype.setItem;
    const setItemSpy = vi
      .spyOn(Storage.prototype, 'setItem')
      .mockImplementation(function (this: Storage, key: string, value: string) {
        if (key === ENTRANCE_STORAGE_KEY) throw new Error('QuotaExceededError');
        originalSetItem.call(this, key, value);
      });
    renderPage(['/entrance']);
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() =>
      expect(screen.getByText('Could not save your choice')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('location')).toHaveTextContent('/entrance');
    setItemSpy.mockRestore();
  });
});
