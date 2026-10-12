import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

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

  it('renders desktop safe inset at native width with untaggable title paint', () => {
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
    expect(inset).toHaveStyle({ width: `${CHRONOS_TITLEBAR_DESKTOP_INSET_PX}px` });

    expect(screen.getByTestId('chronos-titlebar-title')).toHaveClass('select-none');
  });

  it('prevents native image drag in logo slot via draggable={false}', () => {
    render(
      <ChronosTitlebarChrome
        title="Works"
        isDark={false}
        desktopSafeInset
        logo={<img src="/logo.svg" alt="Nexus" draggable={false} />}
      />,
    );

    expect(screen.getByRole('img', { name: 'Nexus' })).toHaveAttribute('draggable', 'false');
  });

  it('omits desktop safe inset in browser mode', () => {
    render(
      <ChronosTitlebarChrome
        title="Works"
        isDark={false}
        desktopSafeInset={false}
        logo={<span>Logo</span>}
      />,
    );

    expect(screen.queryByTestId('chronos-titlebar-desktop-inset')).toBeNull();
    expect(screen.getByTestId('chronos-titlebar-logo-slot')).toBeInTheDocument();
  });

  it('fires onEmptyPaintDoubleClick from safe inset and drag spacer only', async () => {
    const user = userEvent.setup();
    const onEmptyPaintDoubleClick = vi.fn();

    render(
      <ChronosTitlebarChrome
        title="Works"
        isDark={false}
        desktopSafeInset
        onEmptyPaintDoubleClick={onEmptyPaintDoubleClick}
        logo={<button type="button">Logo</button>}
        settingsControl={<button type="button">Gear</button>}
        themeToggle={<button type="button">Theme</button>}
      />,
    );

    await user.dblClick(screen.getByTestId('chronos-titlebar-desktop-inset'));
    await user.dblClick(screen.getByTestId('chronos-titlebar-drag-spacer'));
    expect(onEmptyPaintDoubleClick).toHaveBeenCalledTimes(2);

    onEmptyPaintDoubleClick.mockClear();
    await user.dblClick(screen.getByRole('button', { name: 'Logo' }));
    await user.dblClick(screen.getByRole('button', { name: 'Gear' }));
    await user.dblClick(screen.getByRole('button', { name: 'Theme' }));
    await user.dblClick(screen.getByTestId('chronos-titlebar-title'));
    expect(onEmptyPaintDoubleClick).not.toHaveBeenCalled();
  });
});
