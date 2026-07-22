import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MemoryRouter } from 'react-router-dom';

import { ChronosTitlebarFixtures } from '@/fixtures/chronos-titlebar-fixtures';

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
});
