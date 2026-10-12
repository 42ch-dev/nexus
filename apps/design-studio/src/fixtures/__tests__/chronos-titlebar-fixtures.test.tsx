import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MemoryRouter } from 'react-router';

import { ChronosTitlebarFixtures } from '@/fixtures/chronos-titlebar-fixtures';

function expectDesktopChromeContract(root: HTMLElement) {
  const scope = within(root);

  expect(scope.getByTestId('chronos-titlebar-desktop-inset')).toBeInTheDocument();
  expect(scope.getByTestId('chronos-titlebar-logo-slot')).toBeInTheDocument();
  expect(scope.getByTestId('chronos-titlebar-title')).toHaveClass('select-none');
  expect(scope.getByTestId('chronos-titlebar-drag-spacer')).toBeInTheDocument();
  expect(scope.getByTestId('chronos-titlebar-controls')).toBeInTheDocument();
  expect(scope.getByRole('img', { name: 'Nexus' })).toHaveAttribute('draggable', 'false');
}

describe('ChronosTitlebarFixtures', () => {
  it('renders light and dark titlebar specimens', () => {
    render(
      <MemoryRouter>
        <ChronosTitlebarFixtures />
      </MemoryRouter>,
    );

    expect(screen.getByTestId('chronos-titlebar-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('chronos-titlebar-light')).toBeInTheDocument();
    expect(screen.getByTestId('chronos-titlebar-dark')).toBeInTheDocument();
    expect(screen.getByTestId('chronos-titlebar-dual-pane-light-titlebar')).toBeInTheDocument();
    expect(screen.getAllByTestId('chronos-titlebar-desktop-inset').length).toBeGreaterThan(0);
  });

  it('mirrors desktop chrome contract in light and dark specimens', () => {
    render(
      <MemoryRouter>
        <ChronosTitlebarFixtures />
      </MemoryRouter>,
    );

    expectDesktopChromeContract(screen.getByTestId('chronos-titlebar-drag-contract-light'));
    expectDesktopChromeContract(screen.getByTestId('chronos-titlebar-drag-contract-dark'));
    expectDesktopChromeContract(screen.getByTestId('chronos-titlebar-dual-pane-light-titlebar'));
    expectDesktopChromeContract(screen.getByTestId('chronos-titlebar-dual-pane-dark-titlebar'));
  });

  it('omits the desktop inset in browser-only specimens', () => {
    render(
      <MemoryRouter>
        <ChronosTitlebarFixtures />
      </MemoryRouter>,
    );

    for (const testId of ['chronos-titlebar-light', 'chronos-titlebar-dark']) {
      const scope = within(screen.getByTestId(testId));
      expect(scope.queryByTestId('chronos-titlebar-desktop-inset')).toBeNull();
    }
  });
});
