import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { ReadingProgress } from './reading-progress';

afterEach(cleanup);

function renderProgress(scrollHeight = 2000, clientHeight = 500) {
  const main = document.createElement('main');
  Object.defineProperties(main, {
    scrollHeight: { value: scrollHeight, configurable: true },
    clientHeight: { value: clientHeight, configurable: true },
  });
  document.body.append(main);
  const view = render(<ReadingProgress />, { container: main });
  return { main, ...view };
}

describe('ReadingProgress', () => {
  it('tracks the main scroll container, not the window', async () => {
    const { main } = renderProgress();
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0');

    fireEvent.scroll(main, { target: { scrollTop: 750 } });
    await waitFor(() => expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '50'));

    fireEvent.scroll(main, { target: { scrollTop: 1500 } });
    await waitFor(() => expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '100'));
    fireEvent.scroll(main, { target: { scrollTop: 1600 } });
    await waitFor(() => expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '100'));
  });

  it('keeps non-scrollable content at zero', async () => {
    const { main } = renderProgress(500, 500);
    fireEvent.scroll(main);
    await waitFor(() => expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0'));
  });

  it('recomputes on resize and clamps overscroll', async () => {
    const { main } = renderProgress();
    fireEvent.scroll(main, { target: { scrollTop: 750 } });
    Object.defineProperty(main, 'clientHeight', { value: 1000 });
    fireEvent.resize(window);
    await waitFor(() => expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '75'));
    fireEvent.scroll(main, { target: { scrollTop: -100 } });
    await waitFor(() => expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0'));
  });

  it('stays at zero without a main container', () => {
    render(<ReadingProgress />);
    fireEvent.scroll(window);
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0');
  });
});
