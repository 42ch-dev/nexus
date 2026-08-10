import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MemoryRouter } from 'react-router';

import { ChronosTitlebarFixtures } from '@/fixtures/chronos-titlebar-fixtures';

function expectDesktopDragContract(root: HTMLElement) {
  const scope = within(root);

  expect(scope.getByTestId('chronos-titlebar-desktop-inset')).toHaveAttribute(
    'data-tauri-drag-region',
  );
  expect(scope.getByTestId('chronos-titlebar-logo-slot')).toHaveAttribute(
    'data-tauri-drag-region',
  );
  expect(scope.getByTestId('chronos-titlebar-title')).toHaveAttribute('data-tauri-drag-region');
  expect(scope.getByTestId('chronos-titlebar-title')).toHaveClass('select-none');
  expect(scope.getByTestId('chronos-titlebar-drag-spacer')).toHaveAttribute(
    'data-tauri-drag-region',
  );
  expect(scope.getByTestId('chronos-titlebar-controls')).toHaveAttribute(
    'data-tauri-drag-region',
    'false',
  );
  expect(scope.getByRole('img', { name: 'Nexus' })).toHaveAttribute('draggable', 'false');
  expect(scope.getByRole('button', { name: 'Settings' })).not.toHaveAttribute(
    'data-tauri-drag-region',
  );
  expect(scope.getByRole('button', { name: 'Theme' })).not.toHaveAttribute(
    'data-tauri-drag-region',
  );
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

  it('mirrors desktop drag/no-drag chrome contract in light and dark specimens', () => {
    render(
      <MemoryRouter>
        <ChronosTitlebarFixtures />
      </MemoryRouter>,
    );

    expectDesktopDragContract(screen.getByTestId('chronos-titlebar-drag-contract-light'));
    expectDesktopDragContract(screen.getByTestId('chronos-titlebar-drag-contract-dark'));
    expectDesktopDragContract(screen.getByTestId('chronos-titlebar-dual-pane-light-titlebar'));
    expectDesktopDragContract(screen.getByTestId('chronos-titlebar-dual-pane-dark-titlebar'));
  });

  it('omits drag regions in browser-only specimens', () => {
    render(
      <MemoryRouter>
        <ChronosTitlebarFixtures />
      </MemoryRouter>,
    );

    for (const testId of ['chronos-titlebar-light', 'chronos-titlebar-dark']) {
      const scope = within(screen.getByTestId(testId));
      expect(scope.queryByTestId('chronos-titlebar-desktop-inset')).toBeNull();
      expect(scope.getByTestId('chronos-titlebar-logo-slot')).not.toHaveAttribute(
        'data-tauri-drag-region',
      );
      expect(scope.getByTestId('chronos-titlebar-title')).not.toHaveAttribute(
        'data-tauri-drag-region',
      );
    }
  });
});
