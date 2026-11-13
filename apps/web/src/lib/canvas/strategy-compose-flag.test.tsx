/**
 * `strategy_canvas_compose` flag resolution (v239-p3-canvas-compose-ui T7).
 *
 * Compass D14: default on for the developer entrance, off for the
 * content-creator entrance; an explicit selection overrides either default.
 * D18: the flag is per-surface (Strategy Canvas only) — asserted at the
 * canvas level in strategy-canvas-compose.test.tsx.
 */
import { describe, expect, it } from 'vitest';

import {
  defaultComposeFlagFor,
  resolveComposeFlag,
  COMPOSE_FLAG_STORAGE_KEY,
  useStrategyCanvasComposeFlag,
} from '@/lib/canvas/strategy-compose-flag';
import { renderInApp } from '@/test/test-providers';
import { screen } from '@testing-library/react';

describe('strategy_canvas_compose flag resolution', () => {
  it('defaults on for the developer entrance and off for the content-creator entrance', () => {
    expect(defaultComposeFlagFor('developer')).toBe(true);
    expect(defaultComposeFlagFor('content-creator')).toBe(false);
  });

  it('lets an explicit selection override either audience default', () => {
    expect(resolveComposeFlag('content-creator', 'on')).toBe(true);
    expect(resolveComposeFlag('developer', 'off')).toBe(false);
    expect(resolveComposeFlag('developer', null)).toBe(true);
    expect(resolveComposeFlag('content-creator', null)).toBe(false);
  });

  it('resolves through the entrance when nothing is stored', () => {
    window.localStorage.removeItem(COMPOSE_FLAG_STORAGE_KEY);
    function Probe() {
      return <span data-testid="flag">{useStrategyCanvasComposeFlag() ? 'on' : 'off'}</span>;
    }
    const { unmount } = renderInApp(<Probe />);
    // Test wrapper entrance default is the content-creator entrance.
    expect(screen.getByTestId('flag')).toHaveTextContent('off');
    unmount();

    window.localStorage.setItem(COMPOSE_FLAG_STORAGE_KEY, 'on');
    renderInApp(<Probe />);
    expect(screen.getByTestId('flag')).toHaveTextContent('on');
    window.localStorage.removeItem(COMPOSE_FLAG_STORAGE_KEY);
  });
});
