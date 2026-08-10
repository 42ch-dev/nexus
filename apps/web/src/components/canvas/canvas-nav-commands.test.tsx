/**
 * `CanvasNavCommands` — registers canvas navigation commands into the shared
 * command palette (V1.111 P0 T4).
 *
 * Coverage per task brief: command registration + handler invocation with the
 * router (`useNavigate`) and route-param-derived availability gating.
 *
 * Test strategy: a real `MemoryRouter` (not a mocked `useNavigate`) so the
 * `useParams` → handler → `useNavigate` → `useLocation` loop is exercised
 * end-to-end. A layout route mirrors `RootLayout` so `CanvasNavCommands` stays
 * mounted across child-route changes (which is what the ref-through-handler
 * pattern is for).
 */
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  MemoryRouter,
  Outlet,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from 'react-router-dom';
import { type ReactElement } from 'react';
import { CanvasNavCommands } from '@/components/canvas/canvas-nav-commands';
import { clearCommands, getCommands, type Command } from '@/lib/canvas/command-registry';

/**
 * Records the current pathname via a closure so a test can assert where
 * `useNavigate` landed after a command handler fires. Rendered as a sibling of
 * `<CanvasNavCommands />` in the layout route.
 */
function makeLocationProbe() {
  let last: string | null = null;
  function LocationProbe(): null {
    last = useLocation().pathname;
    return null;
  }
  return {
    LocationProbe,
    read(): string | null {
      return last;
    },
  };
}

/**
 * Renders a button that calls `navigate(target)` so a test can drive an
 * in-router navigation (same `MemoryRouter`, layout stays mounted) without
 * depending on `<Link>` anchor internals.
 */
function makeNavigateButton() {
  function NavigateButton({ to }: { to: string }): ReactElement {
    const navigate = useNavigate();
    return (
      <button type="button" onClick={() => navigate(to)}>
        go-{to}
      </button>
    );
  }
  return NavigateButton;
}

/**
 * Render `CanvasNavCommands` inside a layout route mirroring `RootLayout`, with
 * child routes declared so `useParams` extracts `workId` / `worldId` on the
 * matching child. `initialPath` selects the active child route.
 *
 * When `navigateButtonTarget` is set, a button labelled `go-<target>` is
 * rendered IN THE LAYOUT (always mounted, regardless of which child matches)
 * so a test can drive an in-router navigation to `<target>` without the button
 * unmounting when the child route changes.
 */
function renderInLayout(
  initialPath: string,
  navigateButtonTarget?: string,
) {
  const probe = makeLocationProbe();
  const { LocationProbe } = probe;
  const NavigateButton = makeNavigateButton();

  const result = render(
    <MemoryRouter initialEntries={[initialPath]}>
      <Routes>
        <Route
          element={
            <>
              <CanvasNavCommands />
              <LocationProbe />
              {navigateButtonTarget ? (
                <NavigateButton to={navigateButtonTarget} />
              ) : null}
              <Outlet />
            </>
          }
        >
          <Route path="works/:workId" element={<div />} />
          <Route path="works/:workId/outline" element={<div />} />
          <Route path="works/:workId/timeline" element={<div />} />
          <Route path="works/:workId/inspector" element={<div />} />
          <Route path="worlds/:worldId/kb" element={<div />} />
          <Route path="strategies" element={<div />} />
          <Route path="sessions" element={<div />} />
        </Route>
      </Routes>
    </MemoryRouter>,
  );

  return { ...result, probe };
}

function findById(id: string): Command | undefined {
  return getCommands().find((c) => c.id === id);
}

beforeEach(() => {
  clearCommands();
});

afterEach(() => {
  cleanup();
  clearCommands();
});

describe('CanvasNavCommands — registration', () => {
  // V1.123 P2 Task 5 — `go.work-timeline` registered alongside the V1.111
  // trio (`go.outline`, `go.strategy`, `go.world-kb`). Surgical relaxation of
  // the V1.111 "exactly three" assertion: same Navigate group, same labelKey
  // shape, additive only.
  // V1.123 P3 Task 2 — `go.timeline` (global Timeline entry) registered
  // alongside the V1.123 P2 quartet. Always available; same Navigate group.
  // V1.147 P2 T3 — `compute.run-module` + `compute.run-module-on-world`
  // (jump-only compute commands; the world-scoped one is availability-gated
  // and does not appear on `/sessions`).
  // V1.151 P1 T3 — `go.work-inspector` (Assembly Inspector debug surface;
  // workId-gated like `go.work-timeline`).
  it('registers the Navigate + Compute jump commands (V1.111 trio + Work Timeline + global Timeline + Run Module + Assembly Inspector)', () => {
    renderInLayout('/sessions');
    expect(getCommands().map((c) => c.id).sort()).toEqual([
      'compute.run-module',
      'compute.run-module-on-world',
      'go.outline',
      'go.strategy',
      'go.timeline',
      'go.work-inspector',
      'go.work-timeline',
      'go.world-kb',
    ]);
  });

  it('unregisters all commands on unmount (no leak across mounts)', () => {
    const { unmount } = renderInLayout('/sessions');
    expect(getCommands()).toHaveLength(8);
    unmount();
    expect(getCommands()).toEqual([]);
  });

  it('each command carries a Navigate/Compute group, icon, and non-empty label', () => {
    renderInLayout('/sessions');
    for (const cmd of getCommands()) {
      expect(['group.navigate', 'group.compute']).toContain(cmd.groupKey);
      expect(cmd.labelKey.length).toBeGreaterThan(0);
      expect(cmd.icon).toBeDefined();
    }
  });
});

describe('CanvasNavCommands — Go to Harness (always available)', () => {
  it('is available on an unrelated route', () => {
    renderInLayout('/sessions');
    const cmd = findById('go.strategy');
    expect(cmd?.available?.() ?? true).toBe(true);
  });

  it('navigates to /strategies when invoked', () => {
    const { probe } = renderInLayout('/works/abc');
    act(() => {
      findById('go.strategy')?.handler();
    });
    expect(probe.read()).toBe('/strategies');
  });
});

describe('CanvasNavCommands — Go to Outline (workId-gated)', () => {
  it('is hidden when no workId is in the URL', () => {
    renderInLayout('/sessions');
    expect(findById('go.outline')?.available?.()).toBe(false);
  });

  it('is available and navigates with the workId when on a Work route', () => {
    const { probe } = renderInLayout('/works/w-123');
    const cmd = findById('go.outline');
    expect(cmd?.available?.()).toBe(true);
    act(() => {
      cmd?.handler();
    });
    expect(probe.read()).toBe('/works/w-123/outline');
  });

  it('is available on the outline route itself (workId still present)', () => {
    renderInLayout('/works/w-1/outline');
    expect(findById('go.outline')?.available?.()).toBe(true);
  });

  it('encodes the workId so a space-bearing id stays one path segment', () => {
    // MemoryRouter decodes %20 → space in the param; the handler must
    // re-encode via encodeURIComponent so the navigate target stays valid.
    const { probe } = renderInLayout('/works/w%204');
    act(() => {
      findById('go.outline')?.handler();
    });
    expect(probe.read()).toBe('/works/w%204/outline');
  });
});

describe('CanvasNavCommands — Go to World KB (worldId-gated)', () => {
  it('is hidden when no worldId is in the URL', () => {
    renderInLayout('/works/w-1');
    expect(findById('go.world-kb')?.available?.()).toBe(false);
  });

  it('is available and navigates with the worldId when on a World route', () => {
    const { probe } = renderInLayout('/worlds/world-9/kb');
    const cmd = findById('go.world-kb');
    expect(cmd?.available?.()).toBe(true);
    act(() => {
      cmd?.handler();
    });
    expect(probe.read()).toBe('/worlds/world-9/kb');
  });
});

describe('CanvasNavCommands — Go to Work Timeline (workId-gated, V1.123 P2)', () => {
  // Plan AC-V1123-11: Work Timeline reachable as a peer surface from Work
  // Canvas shell. The command is workId-gated (mirrors `go.outline`); the
  // handler encodes the workId so space-bearing ids stay one path segment.
  it('is hidden when no workId is in the URL', () => {
    renderInLayout('/sessions');
    expect(findById('go.work-timeline')?.available?.()).toBe(false);
  });

  it('is available and navigates to /works/:workId/timeline when on a Work route', () => {
    const { probe } = renderInLayout('/works/w-123');
    const cmd = findById('go.work-timeline');
    expect(cmd?.available?.()).toBe(true);
    act(() => {
      cmd?.handler();
    });
    expect(probe.read()).toBe('/works/w-123/timeline');
  });

  it('is available on the Outline route (workId still present — peer reachable)', () => {
    // Work Timeline is a peer of Outline from the Work Canvas shell; the
    // command MUST stay available while the user is on `/works/:workId/outline`
    // so they can pivot Outline → Work Timeline without going through the list.
    renderInLayout('/works/w-1/outline');
    expect(findById('go.work-timeline')?.available?.()).toBe(true);
  });

  it('encodes the workId so a space-bearing id stays one path segment', () => {
    const { probe } = renderInLayout('/works/w%204');
    act(() => {
      findById('go.work-timeline')?.handler();
    });
    expect(probe.read()).toBe('/works/w%204/timeline');
  });
});

describe('CanvasNavCommands — Go to Assembly Inspector (workId-gated, V1.151 P1 T3)', () => {
  // Plan Q1: the inspector is a moment-level debug surface in the creator
  // area — reachable from ⌘K on Work routes like `go.work-timeline`.
  it('is hidden when no workId is in the URL', () => {
    renderInLayout('/sessions');
    expect(findById('go.work-inspector')?.available?.()).toBe(false);
  });

  it('is available and navigates to /works/:workId/inspector when on a Work route', () => {
    const { probe } = renderInLayout('/works/w-123');
    const cmd = findById('go.work-inspector');
    expect(cmd?.available?.()).toBe(true);
    act(() => {
      cmd?.handler();
    });
    expect(probe.read()).toBe('/works/w-123/inspector');
  });

  it('is available on the Outline route (workId still present — peer reachable)', () => {
    renderInLayout('/works/w-1/outline');
    expect(findById('go.work-inspector')?.available?.()).toBe(true);
  });
});

describe('CanvasNavCommands — live handler reads current id (no remount)', () => {
  it('targets the latest workId after a same-layout route change', () => {
    // useRegisterCommand captures the command once on mount. The handler must
    // read the current workId via the internal ref, so an in-router navigation
    // /works/A → /works/B (re-render of the layout, NOT a remount of
    // CanvasNavCommands) must retarget the handler to B. The layout route stays
    // mounted because only the child changes.
    const { probe } = renderInLayout('/works/A', '/works/B');

    // Initial: handler targets A.
    act(() => {
      findById('go.outline')?.handler();
    });
    expect(probe.read()).toBe('/works/A/outline');

    // Drive an in-router navigation to /works/B (same MemoryRouter, layout
    // route and CanvasNavCommands stay mounted).
    const navButton = screen.getByRole('button', { name: 'go-/works/B' });
    fireEvent.click(navButton);

    // availability still true on B; handler now targets B (proves the ref, not
    // a mount-time closure, is read).
    expect(findById('go.outline')?.available?.()).toBe(true);
    act(() => {
      findById('go.outline')?.handler();
    });
    expect(probe.read()).toBe('/works/B/outline');
  });

  it('flips Outline availability off after navigating to a non-Work route', () => {
    renderInLayout('/works/w-1', '/sessions');
    expect(findById('go.outline')?.available?.()).toBe(true);

    fireEvent.click(screen.getByRole('button', { name: 'go-/sessions' }));

    expect(findById('go.outline')?.available?.()).toBe(false);
  });
});

// (No trailing guard — all imports are used.)


describe('CanvasNavCommands — Compute jumps (V1.147 P2 T3)', () => {
  // Search-aware probe: the compute commands navigate to Settings URLs that
  // carry query params (`/settings/modules?world=…`), so the plain pathname
  // probe is not enough here.
  function makeFullProbe() {
    let last: string | null = null;
    function LocationProbe(): null {
      const location = useLocation();
      last = `${location.pathname}${location.search}`;
      return null;
    }
    return { LocationProbe, read: () => last };
  }

  function renderWithFullProbe(initialPath: string) {
    const probe = makeFullProbe();
    const result = render(
      <MemoryRouter initialEntries={[initialPath]}>
        <Routes>
          <Route
            element={
              <>
                <CanvasNavCommands />
                <probe.LocationProbe />
                <Outlet />
              </>
            }
          >
            <Route path="worlds/:worldId/timeline" element={<div />} />
            <Route path="sessions" element={<div />} />
            {/* Catch-all so Settings deep-link navigations keep the layout
                (and its command registrations + probe) mounted. */}
            <Route path="*" element={<div />} />
          </Route>
        </Routes>
      </MemoryRouter>,
    );
    return { ...result, probe };
  }

  it('Run Module… is always available and jumps to the Modules Run Studio', () => {
    const { probe } = renderWithFullProbe('/sessions');
    const cmd = findById('compute.run-module');
    expect(cmd?.available?.() ?? true).toBe(true);
    act(() => {
      cmd?.handler();
    });
    expect(probe.read()).toBe('/settings/modules');
  });

  it('Run Module on this World… is hidden without a worldId in the URL', () => {
    renderWithFullProbe('/sessions');
    expect(findById('compute.run-module-on-world')?.available?.()).toBe(false);
  });

  it('Run Module on this World… jumps to the Timeline-scoped Run Studio (World pre-filled)', () => {
    const { probe } = renderWithFullProbe('/worlds/world-7/timeline');
    const cmd = findById('compute.run-module-on-world');
    expect(cmd?.available?.()).toBe(true);
    act(() => {
      cmd?.handler();
    });
    expect(probe.read()).toBe('/settings/modules?world=world-7');
  });

  it('encodes the worldId so a space-bearing id stays one query value', () => {
    const { probe } = renderWithFullProbe('/worlds/w%20world/timeline');
    act(() => {
      findById('compute.run-module-on-world')?.handler();
    });
    expect(probe.read()).toBe('/settings/modules?world=w%20world');
  });
});
