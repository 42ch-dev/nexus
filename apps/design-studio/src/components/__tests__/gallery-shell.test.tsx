import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useEffect, useState } from 'react';
import { MemoryRouter, Route, Routes } from 'react-router';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { GalleryShell } from '@/components/gallery-shell';
import * as galleryIndex from '@/lib/gallery-index';

function DelayedCanvasHeading() {
  const [ready, setReady] = useState(false);

  useEffect(() => {
    const timer = window.setTimeout(() => setReady(true), 40);
    return () => window.clearTimeout(timer);
  }, []);

  if (!ready) {
    return (
      <div role="status" aria-busy="true" data-testid="route-loading-canvas">
        Loading Canvas…
      </div>
    );
  }

  return (
    <h3 id="surfaces-canvas-mirrored" tabIndex={-1}>
      Canvas — Three mirrored surfaces + shared chrome
    </h3>
  );
}

describe('GalleryShell discovery focus', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('focuses a sibling Surfaces heading after its delayed route mount', async () => {
    render(
      <MemoryRouter initialEntries={['/surfaces']}>
        <Routes>
          <Route
            path="/surfaces"
            element={
              <GalleryShell>
                <p>Surfaces overview</p>
              </GalleryShell>
            }
          />
          <Route
            path="/surfaces/canvas"
            element={
              <GalleryShell>
                <DelayedCanvasHeading />
              </GalleryShell>
            }
          />
        </Routes>
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole('link', { name: 'Canvas' }));

    const heading = await waitFor(
      () => {
        const element = document.getElementById('surfaces-canvas-mirrored');
        if (!element || document.activeElement !== element) {
          throw new Error('heading not focused yet');
        }
        return element;
      },
      { timeout: 3000 },
    );

    expect(heading).toHaveAttribute('tabindex', '-1');
  });

  it('does not search the parent document for fixture IDs while compare is enabled', async () => {
    const focusSpy = vi.spyOn(galleryIndex, 'focusGalleryHeading');

    render(
      <MemoryRouter initialEntries={['/surfaces/canvas#surfaces-canvas-mirrored']}>
        <Routes>
          <Route
            path="/surfaces/canvas"
            element={
              <GalleryShell>
                <h3 id="surfaces-canvas-mirrored">Canvas</h3>
              </GalleryShell>
            }
          />
        </Routes>
      </MemoryRouter>,
    );

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    const callsBeforeCompare = focusSpy.mock.calls.length;

    fireEvent.click(screen.getByRole('button', { name: 'Compare light/dark' }));

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    expect(focusSpy.mock.calls.length).toBe(callsBeforeCompare);
  });
});
