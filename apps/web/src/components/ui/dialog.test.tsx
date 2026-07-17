import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';

import { Dialog, DialogContent } from './dialog';
import { Sheet, SheetContent } from './sheet';

/**
 * V1.121 P1 T2 — keep-web elevation pass.
 *
 * Pins the v0.4 contract for the Radix-dialog surfaces:
 * - overlay consumes the `scrim` token (never a raw black/N literal);
 * - panels consume `shadow-elevation-4` (modal/drawer tier per §Elevation);
 * - sizing consumes the components.dialog / components.sheet token
 *   projections (w-dialog / max-w-dialog / max-h-dialog / w-sheet) instead of
 *   arbitrary calc/minmax values.
 */
describe('Dialog (v0.4 elevation)', () => {
  it('overlay uses the scrim token and panel uses elevation-4 + dialog sizing tokens', () => {
    const { container } = render(
      <Dialog open>
        <DialogContent title="Delete Work" description="This cannot be undone.">
          <p>Body</p>
        </DialogContent>
      </Dialog>,
    );

    const overlay = container.ownerDocument.querySelector('[data-state="open"]');
    // Radix renders Overlay + Content into a portal on document.body.
    const portalRoot = container.ownerDocument.body;
    const overlayEl = portalRoot.querySelector('.fixed.inset-0');
    expect(overlayEl).not.toBeNull();
    expect(overlayEl!.className).toMatch(/\bbg-scrim\b/);
    expect(overlayEl!.className).not.toMatch(/bg-black\//);
    expect(overlay).not.toBeNull();

    const content = screen.getByRole('dialog');
    expect(content.className).toMatch(/\bshadow-elevation-4\b/);
    expect(content.className).toMatch(/\bw-dialog\b/);
    expect(content.className).toMatch(/\bmax-w-dialog\b/);
    expect(content.className).toMatch(/\bmax-h-dialog\b/);
    // No arbitrary sizing/overlay values remain.
    expect(content.className).not.toMatch(/\[calc\(|\[min\(|max-h-\[85vh\]|max-w-\[560px\]/);
  });
});

describe('Sheet (v0.4 elevation)', () => {
  it('overlay uses the scrim token and panel uses elevation-4 + sheet width token', () => {
    render(
      <Sheet open>
        <SheetContent title="Inspector">
          <p>Rail</p>
        </SheetContent>
      </Sheet>,
    );

    const portalRoot = document.body;
    const overlayEl = portalRoot.querySelector('.fixed.inset-0');
    expect(overlayEl).not.toBeNull();
    expect(overlayEl!.className).toMatch(/\bbg-scrim\b/);

    const content = screen.getByRole('dialog');
    expect(content.className).toMatch(/\bshadow-elevation-4\b/);
    expect(content.className).toMatch(/\bw-sheet\b/);
    expect(content.className).not.toMatch(/\[min\(100vw,280px\)\]/);
  });
});
