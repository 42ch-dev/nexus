import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';

import {
  CHRONOS_TITLEBAR_DESKTOP_INSET_PX,
  ChronosTitlebarChrome,
} from './chronos-titlebar-chrome';

describe('ChronosTitlebarChrome', () => {
  it('paints full-width ink background with theme-aware title labels', () => {
    const { rerender } = render(
      <ChronosTitlebarChrome title="Works" isDark={false} logo={<span>Logo</span>} />,
    );

    const bar = screen.getByTestId('chronos-titlebar');
    expect(bar).toHaveClass('bg-brand-deep-blue');

    const title = screen.getByTestId('chronos-titlebar-title');
    expect(title).toHaveTextContent('Works');
    expect(title).toHaveClass('text-white');

    rerender(
      <ChronosTitlebarChrome title="Works" isDark logo={<span>Logo</span>} />,
    );
    expect(screen.getByTestId('chronos-titlebar-title')).toHaveClass('text-brand-cyan');
  });

  it('reserves desktop traffic-light inset without marking interactive slots draggable', () => {
    render(
      <ChronosTitlebarChrome
        title="Works"
        isDark={false}
        desktopSafeInset
        logo={<button type="button">Logo</button>}
        settingsControl={<button type="button">Gear</button>}
        themeToggle={<button type="button">Theme</button>}
        healthIndicator={<span>Health</span>}
      />,
    );

    const inset = screen.getByTestId('chronos-titlebar-desktop-inset');
    expect(inset).toHaveAttribute('data-tauri-drag-region');
    expect(inset).toHaveStyle({ width: `${CHRONOS_TITLEBAR_DESKTOP_INSET_PX}px` });

    expect(screen.getByTestId('chronos-titlebar-drag-spacer')).toHaveAttribute(
      'data-tauri-drag-region',
    );
    expect(screen.getByTestId('chronos-titlebar-title')).not.toHaveAttribute(
      'data-tauri-drag-region',
    );
    expect(screen.getByRole('button', { name: 'Logo' })).not.toHaveAttribute(
      'data-tauri-drag-region',
    );
    expect(screen.getByRole('button', { name: 'Gear' })).not.toHaveAttribute(
      'data-tauri-drag-region',
    );
  });

  it('omits drag regions in browser mode', () => {
    render(
      <ChronosTitlebarChrome
        title="Works"
        isDark={false}
        desktopSafeInset={false}
      />,
    );

    expect(screen.queryByTestId('chronos-titlebar-desktop-inset')).toBeNull();
    expect(screen.getByTestId('chronos-titlebar-drag-spacer')).not.toHaveAttribute(
      'data-tauri-drag-region',
    );
  });
});
