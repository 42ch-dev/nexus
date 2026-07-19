/**
 * Design Studio smoke tests — T7 + T4 surface-fixture coverage.
 *
 * Coverage:
 *   1. App renders the landing page (HomePage) with section links.
 *   2. Theme toggle switches light ↔ dark and applies the `.dark` class.
 *   3. Each gallery section route renders its heading.
 *   4. Surfaces nested section routes (V1.102 P2) render fixtures with
 *      expected labels and structural elements.
 *
 * These tests catch route/render regressions without snapshot-exhausting every
 * swatch or component variant. Follow apps/web conventions: vitest + jsdom +
 * @testing-library/react.
 */
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { App } from '@/App';
import { ThemeProvider } from '@/components/theme-provider';

/* ---- helpers ------------------------------------------------------------ */

function mockMatchMedia(prefersDark: boolean) {
  const media = {
    matches: prefersDark,
    media: '(prefers-color-scheme: dark)',
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  };
  vi.spyOn(window, 'matchMedia').mockReturnValue(media as unknown as MediaQueryList);
}

function renderStudio(initialRoute = '/') {
  return render(
    <ThemeProvider>
      <MemoryRouter initialEntries={[initialRoute]}>
        <App />
      </MemoryRouter>
    </ThemeProvider>,
  );
}

beforeEach(() => {
  window.localStorage.clear();
  document.documentElement.classList.remove('dark');
  document.documentElement.removeAttribute('data-theme');
});

afterEach(() => {
  vi.restoreAllMocks();
});

/* ---- landing page ------------------------------------------------------- */

describe('App landing page', () => {
  it('renders the studio heading', () => {
    mockMatchMedia(false);
    renderStudio();
    expect(
      screen.getByRole('heading', { name: 'Nexus Design Studio' }),
    ).toBeInTheDocument();
  });

  it('renders the read-only SSOT hint in footer', () => {
    mockMatchMedia(false);
    renderStudio();
    expect(screen.getByText(/Read-only/)).toBeInTheDocument();
  });

  it('renders navigation links for all five gallery sections', () => {
    mockMatchMedia(false);
    renderStudio();
    // Nav links appear in the header AND as home-page cards — verify at
    // least one of each label exists.
    const expectedLabels = ['Tokens', 'Brand', 'Components', 'Voice', 'Surfaces'];
    for (const label of expectedLabels) {
      expect(screen.getAllByText(label).length).toBeGreaterThanOrEqual(1);
    }
  });
});

/* ---- theme toggle ------------------------------------------------------- */

describe('Theme toggle', () => {
  it('starts in light mode when OS prefers light', () => {
    mockMatchMedia(false);
    renderStudio();
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });

  it('starts in dark mode when OS prefers dark', () => {
    mockMatchMedia(true);
    renderStudio();
    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });

  it('toggles dark class on click', () => {
    mockMatchMedia(false);
    renderStudio();

    const toggle = screen.getByLabelText(/Switch to dark theme/);
    expect(document.documentElement.classList.contains('dark')).toBe(false);

    act(() => toggle.click());
    expect(document.documentElement.classList.contains('dark')).toBe(true);

    const toggleDark = screen.getByLabelText(/Switch to light theme/);
    act(() => toggleDark.click());
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });
});

/* ---- gallery section rendering ------------------------------------------ */

const GALLERY_SECTIONS = [
  { route: '/tokens', heading: 'Tokens' },
  { route: '/brand', heading: 'Brand' },
  { route: '/components', heading: 'Components' },
  { route: '/voice', heading: 'Voice & Content' },
  { route: '/surfaces', heading: 'Surfaces' },
] as const;

describe('Gallery section rendering', () => {
  it.each(GALLERY_SECTIONS)(
    'renders $heading at $route',
    ({ route, heading }) => {
      mockMatchMedia(false);
      renderStudio(route);
      // Heading text may appear both in the nav link and the page <h2> —
      // verify at least one instance exists.
      expect(screen.getAllByText(heading).length).toBeGreaterThanOrEqual(1);
    },
  );
});

/* ---- tokens page — V1.121 P0 v0.4 galleries (Task 4) -------------------- */

/** matchMedia mock that distinguishes color-scheme from reduced-motion. */
function mockMatchMediaFull({
  dark = false,
  reducedMotion = false,
}: { dark?: boolean; reducedMotion?: boolean } = {}) {
  vi.spyOn(window, 'matchMedia').mockImplementation(
    (query: string) =>
      ({
        matches: query.includes('reduced-motion') ? reducedMotion : dark,
        media: query,
        onchange: null,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        addListener: vi.fn(),
        removeListener: vi.fn(),
        dispatchEvent: vi.fn(),
      }) as unknown as MediaQueryList,
  );
}

describe('Tokens page — typography gallery (display tier)', () => {
  beforeEach(() => {
    mockMatchMediaFull();
    renderStudio('/tokens');
  });

  it('renders the display tier rows with font-display + text-display-* classes', () => {
    for (const label of ['display-32', 'display-24', 'display-20']) {
      const row = screen.getByTestId(`typo-row-${label}`);
      const specimen = row.querySelector(`.text-${label}`);
      expect(specimen).not.toBeNull();
      expect(specimen!.className).toContain('font-display');
    }
  });

  it('keeps heading specimens in the interface voice (font-sans + font-heading)', () => {
    const row = screen.getByTestId('typo-row-heading-24');
    const specimen = row.querySelector('.text-heading-24');
    expect(specimen).not.toBeNull();
    expect(specimen!.className).toContain('font-sans');
    expect(specimen!.className).toContain('font-heading');
    expect(specimen!.className).not.toContain('font-display');
  });

  it('renders the full sans/mono scale rows', () => {
    for (const label of [
      'heading-32', 'heading-24', 'heading-20', 'heading-16',
      'label-14', 'label-12', 'copy-16', 'copy-14', 'copy-13',
      'button-14', 'button-12', 'label-12-mono', 'copy-13-mono',
    ]) {
      expect(screen.getByTestId(`typo-row-${label}`)).toBeInTheDocument();
    }
  });
});

describe('Tokens page — spacing / radius galleries', () => {
  beforeEach(() => {
    mockMatchMediaFull();
    renderStudio('/tokens');
  });

  it('renders spacing rows driven by the token CSS variable', () => {
    const row = screen.getByTestId('spacing-row-space-4');
    const bar = row.querySelector('[style*="--space-4"]');
    expect(bar).not.toBeNull();
    expect(row).toHaveTextContent('--space-4');
    const row24 = screen.getByTestId('spacing-row-space-24');
    expect(row24.querySelector('[style*="--space-24"]')).not.toBeNull();
    expect(row24).toHaveTextContent('--space-24');
  });

  it('renders radius boxes driven by the token CSS variable', () => {
    const card = screen.getByTestId('radius-box-card');
    expect(card.querySelector('[style*="--radius-card"]')).not.toBeNull();
    expect(card).toHaveTextContent('--radius-card');
    const pill = screen.getByTestId('radius-box-pill');
    expect(pill.querySelector('[style*="--radius-pill"]')).not.toBeNull();
    expect(pill).toHaveTextContent('--radius-pill');
  });
});

describe('Tokens page — elevation gallery', () => {
  beforeEach(() => {
    mockMatchMediaFull();
    renderStudio('/tokens');
  });

  it('renders the full elevation-0…4 scale', () => {
    for (const level of ['elevation-0', 'elevation-1', 'elevation-2', 'elevation-3', 'elevation-4']) {
      expect(screen.getByTestId(`elevation-swatch-${level}`)).toBeInTheDocument();
    }
  });

  it('documents the legacy alias chain onto the scale', () => {
    const aliases = screen.getByTestId('elevation-aliases');
    expect(aliases).toHaveTextContent('shadow-card');
    expect(aliases).toHaveTextContent('→ elevation-1');
    expect(aliases).toHaveTextContent('shadow-popover');
    expect(aliases).toHaveTextContent('→ elevation-3');
    expect(aliases).toHaveTextContent('shadow-modal');
    expect(aliases).toHaveTextContent('→ elevation-4');
  });
});

describe('Tokens page — motion gallery', () => {
  beforeEach(() => {
    mockMatchMediaFull();
    renderStudio('/tokens');
  });

  it('renders duration and easing token rows', () => {
    for (const label of [
      'duration-instant', 'duration-state', 'duration-popover',
      'duration-modal', 'duration-enter', 'duration-exit',
      'ease-standard', 'ease-emphasized',
    ]) {
      expect(screen.getByTestId(`motion-row-${label}`)).toBeInTheDocument();
    }
  });

  it('renders the reduced-motion-aware hover-lift recipe demo', () => {
    const demo = screen.getByTestId('motion-demo-lift');
    expect(demo.className).toContain('shadow-elevation-1');
    expect(demo.className).toContain('hover:shadow-elevation-2');
    expect(demo.className).toContain('duration-popover');
    expect(demo.className).toContain('motion-reduce:transition-none');
  });

  it('toggles the enter/exit demo between duration-enter and duration-exit states', () => {
    vi.useFakeTimers();
    try {
      const chip = screen.getByTestId('motion-demo-enter-exit');
      expect(chip.className).toContain('opacity-100');
      expect(chip.className).toContain('duration-enter');

      fireEvent.click(screen.getByTestId('motion-demo-dismiss'));
      expect(chip.className).toContain('opacity-0');
      expect(chip.className).toContain('duration-exit');

      fireEvent.click(screen.getByTestId('motion-demo-replay'));
      act(() => {
        vi.runAllTimers();
      });
      expect(chip.className).toContain('opacity-100');
      expect(chip.className).toContain('duration-enter');
    } finally {
      vi.useRealTimers();
    }
  });

  it('shows the honesty note when prefers-reduced-motion is active', () => {
    // Re-render with reduced-motion active.
    vi.restoreAllMocks();
    mockMatchMediaFull({ reducedMotion: true });
    renderStudio('/tokens');
    expect(screen.getByTestId('motion-reduced-note')).toBeInTheDocument();
  });

  it('links the Motion section from the token sub-nav', () => {
    const subnav = screen.getByRole('navigation', { name: 'Token sub-sections' });
    expect(within(subnav).getByRole('link', { name: 'Motion' })).toHaveAttribute(
      'href',
      '#tokens-motion',
    );
  });
});

/* ---- surfaces section menu / deep links (V1.102 P2 Task 1) -------------- */

const SURFACES_SECTION_ROUTES = [
  { route: '/surfaces', testId: 'surfaces-index', linkLabel: 'Overview' },
  { route: '/surfaces/setup', testId: 'surfaces-setup', linkLabel: 'Setup' },
  { route: '/surfaces/shell', testId: 'surfaces-shell', linkLabel: 'Shell' },
  {
    route: '/surfaces/agent-picker',
    testId: 'surfaces-agent-picker',
    linkLabel: 'AgentPicker',
  },
  {
    route: '/surfaces/canvas',
    testId: 'surfaces-canvas',
    linkLabel: 'Canvas',
  },
  {
    route: '/surfaces/daemon',
    testId: 'surfaces-daemon',
    linkLabel: 'Daemon',
  },
  {
    route: '/surfaces/launch',
    testId: 'surfaces-launch',
    linkLabel: 'Launch',
  },
  {
    route: '/surfaces/banner',
    testId: 'surfaces-banner',
    linkLabel: 'Banner',
  },
] as const;

describe('Surfaces section menu — deep links', () => {
  it.each(SURFACES_SECTION_ROUTES)(
    'renders $testId at $route with section nav',
    ({ route, testId, linkLabel }) => {
      mockMatchMedia(false);
      renderStudio(route);

      expect(screen.getByTestId(testId)).toBeInTheDocument();
      const sectionNav = screen.getByTestId('surfaces-section-nav');
      expect(sectionNav).toBeInTheDocument();
      expect(
        within(sectionNav).getByRole('link', { name: linkLabel }),
      ).toHaveAttribute('aria-current', 'page');
      // Top gallery Surfaces link stays active for nested section routes.
      const galleryNav = screen.getByRole('navigation', {
        name: 'Gallery sections',
      });
      expect(
        within(galleryNav).getByRole('link', { name: 'Surfaces' }),
      ).toHaveAttribute('aria-current', 'page');
    },
  );

  it('index lists deep links to each Surfaces section', () => {
    mockMatchMedia(false);
    renderStudio('/surfaces');
    const index = screen.getByTestId('surfaces-index');
    expect(within(index).getByRole('link', { name: /Setup/ })).toHaveAttribute(
      'href',
      '/surfaces/setup',
    );
    expect(within(index).getByRole('link', { name: /Shell/ })).toHaveAttribute(
      'href',
      '/surfaces/shell',
    );
    expect(
      within(index).getByRole('link', { name: /AgentPicker/ }),
    ).toHaveAttribute('href', '/surfaces/agent-picker');
    expect(
      within(index).getByRole('link', { name: /Canvas/ }),
    ).toHaveAttribute('href', '/surfaces/canvas');
    expect(within(index).getByRole('link', { name: /Daemon/ })).toHaveAttribute(
      'href',
      '/surfaces/daemon',
    );
    expect(within(index).getByRole('link', { name: /Launch/ })).toHaveAttribute(
      'href',
      '/surfaces/launch',
    );
    expect(within(index).getByRole('link', { name: /Banner/ })).toHaveAttribute(
      'href',
      '/surfaces/banner',
    );
  });
});

/* ---- surfaces page fixtures (T4) ---------------------------------------- */

describe('Surfaces page — setup wizard chrome fixtures', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/surfaces/setup');
  });

  it('renders the wizard chrome section heading', () => {
    expect(
      screen.getByRole('heading', { name: 'Setup — Wizard chrome' }),
    ).toBeInTheDocument();
  });

  it('covers agent / workspace / done step matrices plus overflow', () => {
    expect(screen.getByTestId('wizard-chrome-steps-agent')).toBeInTheDocument();
    expect(screen.getByTestId('wizard-chrome-steps-workspace')).toBeInTheDocument();
    expect(screen.getByTestId('wizard-chrome-steps-done')).toBeInTheDocument();
    expect(screen.getByTestId('wizard-chrome-steps-agent-overflow')).toBeInTheDocument();
  });

  it('uses portrait shell classes without left step rail', () => {
    const card = screen.getByTestId('wizard-chrome-card-agent');
    expect(card).toHaveAttribute('data-shell', 'portrait');
    expect(card).toHaveClass(
      'max-w-setup-wizard-step-wizard-max-width',
      'h-setup-wizard-wizard-max-height',
      'max-h-[85vh]',
    );
    expect(card.querySelector('.w-setup-wizard-surface-step-panel-width')).toBeNull();
    expect(card.querySelector('[data-testid="top-step-indicator"]')).toBeInTheDocument();
  });

  it('maps step statuses for the workspace-active matrix', () => {
    const card = screen.getByTestId('wizard-chrome-card-workspace');
    expect(card.querySelector('[data-step-id="agent"]')).toHaveAttribute(
      'data-step-status',
      'complete',
    );
    expect(card.querySelector('[data-step-id="workspace"]')).toHaveAttribute(
      'data-step-status',
      'active',
    );
    expect(card.querySelector('[data-step-id="done"]')).toHaveAttribute(
      'data-step-status',
      'pending',
    );
  });

  it('shows numbered step circles (1–3) on the agent fixture', () => {
    const card = screen.getByTestId('wizard-chrome-card-agent');
    const circles = card.querySelectorAll('[data-step-id] span.rounded-full');
    expect(circles).toHaveLength(3);
    expect(circles[0]).toHaveTextContent('1');
    expect(circles[1]).toHaveTextContent('2');
    expect(circles[2]).toHaveTextContent('3');
  });

  it('uses a single horizontal Back / Continue CTA row on workspace', () => {
    const workspaceCard = screen.getByTestId('wizard-chrome-card-workspace');
    const cta = workspaceCard.querySelector('[data-testid="wizard-cta-row"]');
    expect(cta).toHaveAttribute('data-layout', 'horizontal-adjacent');
    expect(cta).toHaveClass('flex', 'items-center');
    expect(cta).not.toHaveClass('flex-col');

    const back = cta?.querySelector('button[aria-label="Back"]');
    expect(back).toBeInTheDocument();
    expect(back).not.toHaveTextContent('Back');
    expect(cta?.querySelectorAll('button')[1]).toHaveTextContent('Continue');
  });

  it('omits Back on agent; shows Finish on done', () => {
    const agentCard = screen.getByTestId('wizard-chrome-card-agent');
    expect(
      agentCard.querySelector('[data-testid="wizard-cta-row"] button[aria-label="Back"]'),
    ).not.toBeInTheDocument();

    const doneCard = screen.getByTestId('wizard-chrome-card-done');
    expect(
      doneCard.querySelector('[data-testid="wizard-cta-row"] button[aria-label="Back"]'),
    ).toBeInTheDocument();
    expect(doneCard.querySelector('[data-testid="wizard-cta-row"]')).toHaveTextContent(
      'Open Nexus',
    );
  });

  it('scrolls long agent lists inside the portrait card', () => {
    const overflowCard = screen.getByTestId('wizard-chrome-steps-agent-overflow');
    const body = overflowCard.querySelector('[data-testid="wizard-step-body"]');
    expect(body).toHaveClass('overflow-y-auto', 'min-h-0', 'flex-1');
    const grid = overflowCard.querySelector('[data-testid="agent-picker-grid"]');
    expect(grid).toBeInTheDocument();
    expect(grid!.children.length).toBeGreaterThan(6);
    expect(overflowCard.querySelector('[data-testid="wizard-cta-row"]')).toBeInTheDocument();
  });

  it('mounts the shared AgentPicker with data-status reflecting the fixture state', () => {
    const agentCard = screen.getByTestId('wizard-chrome-steps-agent');
    const picker = within(agentCard).getByTestId('agent-picker');
    expect(picker).toHaveAttribute('data-status', 'ready');

    const loadingCard = screen.getByTestId('wizard-chrome-agent-loading');
    expect(within(loadingCard).getByTestId('agent-picker')).toHaveAttribute(
      'data-status',
      'loading',
    );

    const emptyCard = screen.getByTestId('wizard-chrome-agent-empty');
    expect(within(emptyCard).getByTestId('agent-picker')).toHaveAttribute('data-status', 'empty');

    const errorCard = screen.getByTestId('wizard-chrome-agent-error');
    expect(within(errorCard).getByTestId('agent-picker')).toHaveAttribute('data-status', 'error');
  });

  it('renders horizontal connectors between top steps', () => {
    const card = screen.getByTestId('wizard-chrome-card-agent');
    const connectors = card.querySelectorAll('[data-testid="step-connector"]');
    expect(connectors.length).toBe(2);
    connectors.forEach((el) => {
      expect(el).toHaveClass('bg-setup-wizard-step-connector', 'h-px');
    });
  });
});

describe('Surfaces page — app shell fixture', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/surfaces/shell');
  });

  it('renders Creator and Orchestrator tabs', () => {
    // App shell + Settings host fixtures both stub Creator/Orchestrator tabs.
    expect(screen.getAllByText('Creator').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('Orchestrator').length).toBeGreaterThanOrEqual(1);
  });

  it('renders Works nav group and All Works child', () => {
    // "Works" appears as a nav group label — verify at least one instance.
    expect(screen.getAllByText('Works').length).toBeGreaterThanOrEqual(1);
    // Both the app shell fixture and Settings fixture use ShellSidebarChrome
    // SSOT, so "All Works" renders in each.
    expect(screen.getAllByText('All Works').length).toBeGreaterThanOrEqual(1);
  });

  it('renders Settings fixture sidebar with SSOT segmented pill tabs (FB-UI-002)', () => {
    // The Settings fixture consumes ShellSidebarChrome — segmented pill
    // tablist, not stale underline tabs.
    const settingsShell = screen.getByTestId('settings-shell-chrome');
    const tablist = within(settingsShell).getByRole('tablist', {
      name: 'Primary navigation',
    });
    expect(
      within(tablist).getByRole('tab', { name: 'Creator' }),
    ).toHaveAttribute('aria-selected', 'true');
    expect(
      within(tablist).getByRole('tab', { name: 'Orchestrator' }),
    ).toHaveAttribute('aria-selected', 'false');
  });

  it('renders Settings fixture sidebar with sectioned icon nav (FB-UI-003)', () => {
    const settingsShell = screen.getByTestId('settings-shell-chrome');
    // Creator nav groups render as section headers + icon+label items.
    expect(within(settingsShell).getByText('Memory')).toBeInTheDocument();
  });

  it('renders Settings fixture profiles as icon-only (FB-UI-001)', () => {
    const settingsShell = screen.getByTestId('settings-shell-chrome');
    const toolbar = within(settingsShell).getByRole('toolbar', {
      name: 'Profiles',
    });
    expect(toolbar).toBeInTheDocument();
    // Icon-only — no display name text visible in the settings shell.
    expect(
      within(settingsShell).queryByText('Local Creator'),
    ).not.toBeInTheDocument();
  });

  it('renders the profile footer with creator name', () => {
    expect(screen.getAllByText('Local Creator').length).toBeGreaterThanOrEqual(1);
  });

  it('renders add-profile button with accessible label', () => {
    const appShell = screen.getByTestId('app-shell-fixture');
    expect(
      within(appShell).getByRole('button', { name: 'Add profile' }),
    ).toBeInTheDocument();
  });

  it('renders content panel placeholder', () => {
    expect(screen.getByText('Content panel')).toBeInTheDocument();
  });
});

describe('Surfaces page — daemon status strip', () => {
  it('renders single-line status + soft badge + Restart (no description)', () => {
    mockMatchMedia(false);
    renderStudio('/surfaces/daemon');

    const strip = screen.getByTestId('daemon-status-strip');
    expect(within(strip).getByText('Daemon running')).toBeInTheDocument();
    expect(within(strip).getByText('healthy')).toBeInTheDocument();
    expect(
      within(strip).getByRole('button', { name: 'Restart daemon' }),
    ).toBeInTheDocument();
    expect(
      within(strip).queryByText(/Daemon API is reachable/i),
    ).not.toBeInTheDocument();
  });
});

/* ---- surfaces page — launch splash fixtures (V1.106 P0 Task 3) --------- */

describe('Surfaces page — launch splash fixtures', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/surfaces/launch');
  });

  it('renders the launch section heading', () => {
    expect(
      screen.getByRole('heading', { name: 'Launch — Daemon splash' }),
    ).toBeInTheDocument();
  });

  it('renders the page root and three daemon-ready-splash mounts', () => {
    expect(screen.getByTestId('surfaces-launch')).toBeInTheDocument();
    const mounts = screen.getAllByTestId('daemon-ready-splash');
    expect(mounts).toHaveLength(3);
  });

  it('covers waiting, error+retry, and reset-local-database variants', () => {
    expect(screen.getByText('Starting daemon…')).toBeInTheDocument();
    expect(screen.getAllByText('Daemon not ready').length).toBeGreaterThanOrEqual(2);
    expect(
      screen.getAllByRole('button', { name: 'Restart' }).length,
    ).toBeGreaterThanOrEqual(2);
    expect(screen.getByText('Reset')).toBeInTheDocument();
  });
});

/* ---- surfaces page — main banner fixtures (V1.106 P0 Task 3) ----------- */

describe('Surfaces page — main banner fixtures', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/surfaces/banner');
  });

  it('renders the banner section heading', () => {
    expect(
      screen.getByRole('heading', { name: 'Launch — Daemon banner' }),
    ).toBeInTheDocument();
  });

  it('renders all four main banner fixture variants', () => {
    expect(screen.getByTestId('main-banner-fixture-starting')).toBeInTheDocument();
    expect(screen.getByTestId('main-banner-fixture-degraded')).toBeInTheDocument();
    expect(screen.getByTestId('main-banner-fixture-stopped')).toBeInTheDocument();
    expect(screen.getByTestId('main-banner-fixture-error')).toBeInTheDocument();
  });

  it('shows port-conflict copy on the error variant', () => {
    const errorFixture = screen.getByTestId('main-banner-fixture-error');
    expect(within(errorFixture).getByText('Port unavailable')).toBeInTheDocument();
    expect(
      within(errorFixture).getByText(/Port 8420 is already in use/i),
    ).toBeInTheDocument();
  });

  it('does not import the App MainBanner (composition-only)', () => {
    // The fixture exposes stable, state-specific testids built from inline
    // markup and @42ch/nexus-ui Button. The real apps/web banner has no
    // data-testid and renders a single dynamic daemon state, so four matching
    // fixture roots prove the fixture is used and the App banner is not
    // imported.
    const bannerSection = screen.getByTestId('surfaces-banner');
    expect(
      within(bannerSection).getByTestId('main-banner-fixture-starting'),
    ).toBeInTheDocument();
    expect(
      within(bannerSection).getByTestId('main-banner-fixture-degraded'),
    ).toBeInTheDocument();
    expect(
      within(bannerSection).getByTestId('main-banner-fixture-stopped'),
    ).toBeInTheDocument();
    expect(
      within(bannerSection).getByTestId('main-banner-fixture-error'),
    ).toBeInTheDocument();

    // The real MainBanner does not emit any data-testid; an imported copy
    // would add an uncontrolled root element without the fixture prefix.
    expect(
      within(bannerSection).queryAllByTestId(/^main-banner-/).length,
    ).toBe(4);
  });
});

/* ---- components page — Toast matrix (V1.106 P0 Task 3) ---------------- */

describe('Components page — Toast matrix', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/components');
  });

  it('renders the Toast section heading and fixture root', () => {
    expect(screen.getByRole('heading', { name: 'Toast' })).toBeInTheDocument();
    expect(screen.getByTestId('toast-matrix')).toBeInTheDocument();
  });

  it('renders all four toast variant testids', () => {
    expect(screen.getByTestId('toast-variant-success')).toBeInTheDocument();
    expect(screen.getByTestId('toast-variant-error')).toBeInTheDocument();
    expect(screen.getByTestId('toast-variant-warning')).toBeInTheDocument();
    expect(screen.getByTestId('toast-variant-info')).toBeInTheDocument();
  });

  it('uses error role on the error variant and status on others', () => {
    expect(screen.getByTestId('toast-variant-error')).toHaveAttribute('role', 'alert');
    expect(screen.getByTestId('toast-variant-success')).toHaveAttribute('role', 'status');
    expect(screen.getByTestId('toast-variant-warning')).toHaveAttribute('role', 'status');
    expect(screen.getByTestId('toast-variant-info')).toHaveAttribute('role', 'status');
  });
});

/* ---- surfaces page — AgentPicker fixtures (V1.101 P0 Task 2) ------------ */

describe('Surfaces page — AgentPicker fixtures', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/surfaces/agent-picker');
  });

  it('renders the AgentPicker section heading', () => {
    expect(
      screen.getByRole('heading', { name: 'Setup — AgentPicker' }),
    ).toBeInTheDocument();
  });

  it('covers loading, empty, and error states', () => {
    expect(screen.getByText('Scanning for local agents…')).toBeInTheDocument();
    // Multiple empty-status fixtures (Empty + Verify matrix) render this title.
    expect(screen.getAllByText('No agents found on PATH').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('Could not scan for agents')).toBeInTheDocument();
  });

  it('shows custom launch on empty and error', () => {
    const customFields = screen.getAllByTestId('agent-picker-custom-launch');
    expect(customFields.length).toBeGreaterThanOrEqual(2);
  });

  it('renders installed and not-installed cards in mixed fixture', () => {
    const installed = screen.getAllByTestId('agent-card-claude-acp');
    expect(installed.length).toBeGreaterThanOrEqual(1);
    expect(installed[0]).toHaveAttribute('data-installed', 'true');

    const missing = screen.getAllByTestId('agent-card-kimi');
    expect(missing.length).toBeGreaterThanOrEqual(1);
    expect(missing[0]).toHaveAttribute('data-installed', 'false');
  });

  it('hides outbound links when URLs are missing (Cursor Agent)', () => {
    const hiddenLinksCard = screen.getByTestId('agent-card-cursor');
    expect(hiddenLinksCard.querySelector('a')).toBeNull();
  });

  it('shows Install outbound link when URL is present', () => {
    expect(screen.getAllByRole('link', { name: /Install/i }).length).toBeGreaterThanOrEqual(1);
  });

  it('keeps Install links outside select buttons', () => {
    const selects = screen.getAllByTestId(/^agent-card-select-/);
    for (const select of selects) {
      expect(select.querySelector('a')).toBeNull();
    }
  });

  it('marks selected fixture with aria-pressed', () => {
    const pressed = screen
      .getAllByTestId('agent-card-select-claude-acp')
      .filter((el) => el.getAttribute('aria-pressed') === 'true');
    expect(pressed.length).toBeGreaterThanOrEqual(1);
  });

  it('renders the V1.108 Verify Agent static state matrix (FB-UI-008)', () => {
    // idle: Verify button visible and enabled (command non-empty).
    const idleBtns = screen.getAllByTestId('agent-picker-verify');
    expect(idleBtns.length).toBeGreaterThanOrEqual(1);

    const idleVerify = idleBtns.find((b) => b.textContent === 'Verify');
    expect(idleVerify).toBeTruthy();

    // loading: Verifying… label + disabled.
    const loadingVerify = idleBtns.find((b) => b.textContent === 'Verifying…');
    expect(loadingVerify).toBeTruthy();
    expect(loadingVerify).toBeDisabled();

    // success helper.
    expect(screen.getByText('Agent responded successfully.')).toBeInTheDocument();

    // failure helper.
    expect(
      screen.getByText('Could not reach this agent. Check the command and try again.'),
    ).toBeInTheDocument();
  });
});

/* ---- surfaces page — Settings shell chrome (V1.103 P0 Task 2) ----------- */

describe('Surfaces page — Settings shell chrome fixtures', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    // Settings shell chrome remains discoverable under Shell (Studio-only).
    renderStudio('/surfaces/shell');
  });

  it('renders the Settings shell chrome section heading', () => {
    expect(
      screen.getByRole('heading', { name: 'Settings — Shell chrome' }),
    ).toBeInTheDocument();
  });

  it('renders shell chrome with footer utility Settings link', () => {
    const settingsShell = screen.getByTestId('settings-shell-chrome');
    expect(settingsShell).toBeInTheDocument();
    const link = within(settingsShell).getByTestId('settings-footer-utility-link');
    expect(link).toHaveTextContent('Settings');
    expect(link).toHaveAttribute('aria-current', 'page');
  });

  it('renders section nav with Agent, Workspace, Advanced', () => {
    const hostRoot = screen.getByTestId('settings-host-fixtures');
    const sectionNav = within(hostRoot).getByTestId('settings-section-nav');
    expect(
      within(sectionNav).getByTestId('settings-section-nav-agent'),
    ).toHaveTextContent('Agent');
    expect(
      within(sectionNav).getByTestId('settings-section-nav-workspace'),
    ).toHaveTextContent('Workspace');
    expect(
      within(sectionNav).getByTestId('settings-section-nav-advanced'),
    ).toHaveTextContent('Advanced');
    expect(
      within(sectionNav).queryByTestId('settings-section-nav-connection'),
    ).not.toBeInTheDocument();
    expect(
      within(sectionNav).queryByTestId('settings-section-nav-setup'),
    ).not.toBeInTheDocument();
  });

  it('defaults to Agent section with preselected Agent body and locked shell helper', () => {
    const hostRoot = screen.getByTestId('settings-host-fixtures');
    const shellPages = within(hostRoot).getAllByTestId(
      'settings-shell-page-chrome',
    );
    expect(shellPages.length).toBeGreaterThanOrEqual(1);
    expect(
      within(hostRoot).getByTestId('settings-section-nav-agent'),
    ).toHaveAttribute('aria-current', 'page');
    const outlet = within(hostRoot).getByTestId('settings-shell-outlet');
    const agentSection = within(outlet).getByTestId('settings-agent-section');
    expect(agentSection).toHaveAttribute('data-preselected', 'codex-native');
    expect(
      within(outlet).queryByTestId('settings-section-frame-agent'),
    ).not.toBeInTheDocument();
    expect(
      within(hostRoot).getAllByText(
        /Manage your local agent, workspace, daemon connection, and setup options/i,
      ).length,
    ).toBeGreaterThanOrEqual(1);
    // Shell must not include wizard Back/Continue chrome (wizard CTAs live
    // on /surfaces/setup).
    expect(
      within(hostRoot).queryByTestId('wizard-cta-row'),
    ).not.toBeInTheDocument();
  });

  it('switches to Advanced section chrome when section nav is clicked', () => {
    const hostRoot = screen.getByTestId('settings-host-fixtures');
    const outlet = within(hostRoot).getByTestId('settings-shell-outlet');
    const advancedTab = within(hostRoot).getByTestId(
      'settings-section-nav-advanced',
    );
    fireEvent.click(advancedTab);
    expect(advancedTab).toHaveAttribute('aria-current', 'page');
    expect(
      within(outlet).getByTestId('settings-advanced-section'),
    ).toBeInTheDocument();
    expect(
      within(outlet).getByTestId('settings-connection-section'),
    ).toBeInTheDocument();
    expect(
      within(outlet).getByTestId('settings-setup-section'),
    ).toBeInTheDocument();
    expect(
      within(outlet).queryByTestId('settings-section-frame-connection'),
    ).not.toBeInTheDocument();
    expect(
      within(outlet).queryByTestId('settings-section-frame-setup'),
    ).not.toBeInTheDocument();
    expect(
      within(outlet).queryByTestId('settings-agent-section'),
    ).not.toBeInTheDocument();
  });

  it('renders static empty frames for all four Must sections', () => {
    const framesRoot = screen.getByTestId(
      'settings-host-fixture-section-frames',
    );
    expect(
      within(framesRoot).getByTestId('settings-section-frame-agent'),
    ).toBeInTheDocument();
    expect(
      within(framesRoot).getByTestId('settings-section-frame-connection'),
    ).toBeInTheDocument();
    expect(
      within(framesRoot).getByTestId('settings-section-frame-setup'),
    ).toBeInTheDocument();
    expect(
      within(framesRoot).getByTestId('settings-section-frame-workspace'),
    ).toBeInTheDocument();
  });

  it('renders Agent section fixture with locked helper, preselected Codex, and Save Agent', () => {
    const agentRoot = screen.getByTestId(
      'settings-host-fixture-agent-section',
    );
    const section = within(agentRoot).getByTestId('settings-agent-section');
    expect(section).toHaveAttribute('data-preselected', 'codex-native');
    expect(
      within(agentRoot).getByText(
        /Choose which local ACP agent Nexus uses for creative work/i,
      ),
    ).toBeInTheDocument();
    const pressed = within(agentRoot)
      .getAllByTestId('agent-card-select-codex-native')
      .filter((el) => el.getAttribute('aria-pressed') === 'true');
    expect(pressed.length).toBeGreaterThanOrEqual(1);
    // Preselect is Codex, not the first-installed Claude default.
    const claudePressed = within(agentRoot)
      .getAllByTestId('agent-card-select-claude-native')
      .filter((el) => el.getAttribute('aria-pressed') === 'true');
    expect(claudePressed.length).toBe(0);
    expect(
      within(agentRoot).getByTestId('settings-save-agent'),
    ).toHaveTextContent('Save Agent');
  });

  it('renders Connection section fixture with locked helper and form chrome', () => {
    const connectionRoot = screen.getByTestId(
      'settings-host-fixture-connection-section',
    );
    expect(
      within(connectionRoot).getByTestId('settings-connection-section'),
    ).toBeInTheDocument();
    expect(
      within(connectionRoot).getByText(
        /Connect this app to a remote Nexus daemon\. Your local daemon stays the default/i,
      ),
    ).toBeInTheDocument();
    const form = within(connectionRoot).getByTestId(
      'settings-connection-form-chrome',
    );
    expect(within(form).getByText('Connect to Daemon')).toBeInTheDocument();
    expect(
      within(form).getByText(
        /Enter the remote daemon URL and API key\. Local mode remains available/i,
      ),
    ).toBeInTheDocument();
    expect(
      within(form).getByText(
        /The full HTTPS address of the daemon, including port/i,
      ),
    ).toBeInTheDocument();
    expect(
      within(form).getByText(/The API key from the daemon machine/i),
    ).toBeInTheDocument();
    expect(
      within(form).getByText('nexus42 daemon api-key'),
    ).toBeInTheDocument();
    expect(
      within(form).getByText(
        /Confirm the certificate fingerprint matches what you see on the daemon machine/i,
      ),
    ).toBeInTheDocument();
    expect(
      within(form).getByTestId('trust-connect-button'),
    ).toHaveTextContent('Reconnect With These Settings');
    expect(
      within(form).getByTestId('revert-local-button'),
    ).toHaveTextContent('Use Local Daemon');
  });

  it('renders Setup section fixture with locked helper and Re-run Setup CTA', () => {
    const setupRoot = screen.getByTestId(
      'settings-host-fixture-setup-section',
    );
    const section = within(setupRoot).getByTestId('settings-setup-section');
    expect(section).toHaveAttribute('data-desktop', 'true');
    expect(
      within(setupRoot).getByText(
        /Return to the first-run wizard to walk through setup steps again\. Your workspace and agent choices are kept/i,
      ),
    ).toBeInTheDocument();
    expect(
      within(setupRoot).getByTestId('settings-rerun-setup'),
    ).toHaveTextContent('Re-run');
  });

  it('opens Re-run Setup confirm dialog with locked copy and Title Case CTAs', () => {
    const setupRoot = screen.getByTestId(
      'settings-host-fixture-setup-section',
    );
    fireEvent.click(within(setupRoot).getByTestId('settings-rerun-setup'));
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText('Re-run Setup?')).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        /This restarts the setup wizard from the beginning\. Your workspace path and agent profile are not deleted/i,
      ),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByTestId('settings-rerun-setup-cancel'),
    ).toHaveTextContent('Cancel');
    expect(
      within(dialog).getByTestId('settings-rerun-setup-confirm-action'),
    ).toHaveTextContent('Re-run');
    // Close so Radix aria-hidden does not leak into later tests in this suite.
    fireEvent.click(within(dialog).getByTestId('settings-rerun-setup-cancel'));
  });

  it('renders Setup confirm dialog fixture open for visual acceptance', () => {
    const confirmRoot = screen.getByTestId(
      'settings-host-fixture-setup-confirm',
    );
    const chrome = within(confirmRoot).getByTestId(
      'settings-rerun-setup-confirm-chrome',
    );
    expect(within(chrome).getByText('Re-run Setup?')).toBeInTheDocument();
    expect(
      within(chrome).getByText(
        /This restarts the setup wizard from the beginning\. Your workspace path and agent profile are not deleted/i,
      ),
    ).toBeInTheDocument();
    expect(within(chrome).getByText('Cancel')).toBeInTheDocument();
    expect(within(chrome).getByText('Re-run')).toBeInTheDocument();
  });

  it('renders Setup browser-only fixture with disabled CTA and honest helper', () => {
    const browserRoot = screen.getByTestId(
      'settings-host-fixture-setup-browser',
    );
    const section = within(browserRoot).getByTestId('settings-setup-section');
    expect(section).toHaveAttribute('data-desktop', 'false');
    expect(
      within(browserRoot).getByText(
        /Re-run setup is available on the desktop app only/i,
      ),
    ).toBeInTheDocument();
    const cta = within(browserRoot).getByTestId('settings-rerun-setup');
    expect(cta).toBeDisabled();
    expect(cta).toHaveAttribute(
      'title',
      'Open the Nexus desktop app to re-run setup.',
    );
  });

  it('switches to Workspace section chrome when section nav is clicked', () => {
    const hostRoot = screen.getByTestId('settings-host-fixtures');
    const outlet = within(hostRoot).getByTestId('settings-shell-outlet');
    const workspaceTab = within(hostRoot).getByTestId(
      'settings-section-nav-workspace',
    );
    fireEvent.click(workspaceTab);
    expect(workspaceTab).toHaveAttribute('aria-current', 'page');
    expect(
      within(outlet).getByTestId('settings-workspace-section'),
    ).toBeInTheDocument();
    expect(
      within(outlet).queryByTestId('settings-section-frame-workspace'),
    ).not.toBeInTheDocument();
    expect(
      within(outlet).queryByTestId('settings-agent-section'),
    ).not.toBeInTheDocument();
  });

  it('renders Workspace section fixture with locked helper, path, and Change Folder CTA', () => {
    const workspaceRoot = screen.getByTestId(
      'settings-host-fixture-workspace-section',
    );
    const section = within(workspaceRoot).getByTestId('settings-workspace-section');
    expect(section).toHaveAttribute('data-desktop', 'true');
    expect(
      within(workspaceRoot).getByText(
        /View or change where Nexus stores your creative files on this machine/i,
      ),
    ).toBeInTheDocument();
    expect(
      within(workspaceRoot).getByTestId('settings-workspace-path'),
    ).toHaveValue('/Users/creator/Documents/Nexus');
    const cta = within(workspaceRoot).getByTestId('settings-change-folder');
    expect(cta).toHaveTextContent('Change…');
    expect(cta).not.toBeDisabled();
  });

  it('renders Workspace post-persist fixture with honesty copy', () => {
    const savedRoot = screen.getByTestId('settings-host-fixture-workspace-saved');
    const section = within(savedRoot).getByTestId('settings-workspace-section');
    expect(section).toHaveAttribute('data-desktop', 'true');
    expect(
      within(savedRoot).getByTestId('settings-workspace-saved-honesty'),
    ).toHaveTextContent(
      'Workspace path saved. Restart or reload the app so the running daemon uses the new location.',
    );
    expect(
      within(savedRoot).getByText('Quit and reopen Nexus'),
    ).toBeInTheDocument();
  });

  it('renders Workspace browser-only fixture with disabled CTA and honest helper', () => {
    const browserRoot = screen.getByTestId(
      'settings-host-fixture-workspace-browser',
    );
    const section = within(browserRoot).getByTestId('settings-workspace-section');
    expect(section).toHaveAttribute('data-desktop', 'false');
    expect(
      within(browserRoot).getByText(
        /Workspace path changes are available on the desktop app only/i,
      ),
    ).toBeInTheDocument();
    const cta = within(browserRoot).getByTestId('settings-change-folder');
    expect(cta).toBeDisabled();
    expect(cta).toHaveAttribute(
      'title',
      'Open the Nexus desktop app to change your workspace folder.',
    );
  });

  it('retains AgentPicker thin-host reference for P1', () => {
    const regions = screen.getAllByTestId('settings-host-picker-region');
    expect(regions.length).toBeGreaterThanOrEqual(1);
    const cards = screen.getAllByTestId('agent-card-claude-acp');
    expect(cards.length).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByTestId('agent-picker-custom-launch').length,
    ).toBeGreaterThanOrEqual(1);
  });
});

/* ---- components page — form-field composition fixture (P2 T3) ----------- */

describe('Components page — form-field composition fixture', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/components');
  });

  it('renders the form-field composition section heading', () => {
    expect(
      screen.getByRole('heading', { name: 'Form Field (composition)' }),
    ).toBeInTheDocument();
  });

  it('demonstrates label/control association via htmlFor/id', () => {
    const label = screen.getByText('Work title');
    expect(label.tagName).toBe('LABEL');
    expect(label).toHaveAttribute('for', 'ff-name');

    const input = screen.getByPlaceholderText('Enter work title…');
    expect(input).toHaveAttribute('id', 'ff-name');
    expect(input).toHaveAttribute('aria-describedby');
  });

  it('renders helper text with app-owned id', () => {
    const helper = screen.getByText('Must be between 3 and 50 characters.');
    expect(helper).toHaveAttribute('id', 'ff-name-helper');
  });

  it('does not show error message initially', () => {
    // ErrorState from StatesSection also has role="alert" — check specifically
    // that our form-field error element (ff-name-error) is absent.
    expect(screen.queryByText('Name is required.')).not.toBeInTheDocument();
  });

  it('shows error message with role="alert" after triggering error', () => {
    const btn = screen.getByRole('button', { name: 'Trigger error' });
    act(() => btn.click());

    // There are two role="alert" on the page (ErrorState + our triggered error).
    // Query specifically for our error element by id.
    const error = document.getElementById('ff-name-error');
    expect(error).toBeInTheDocument();
    expect(error).toHaveTextContent('Name is required.');
    expect(error).toHaveAttribute('role', 'alert');
  });

  it('renders required field indicator (*)', () => {
    expect(screen.getByText('*')).toBeInTheDocument();
    // The required input should have the required attribute
    const emailInput = screen.getByPlaceholderText('you@example.com');
    expect(emailInput).toBeRequired();
  });

  it('renders optional field indicator', () => {
    expect(screen.getByText('(optional)')).toBeInTheDocument();
  });

  it('renders disabled textarea', () => {
    const bio = screen.getByPlaceholderText('Tell us about yourself…');
    expect(bio).toBeDisabled();
  });
});

/* ---- components page — Badge soft/solid matrix (V1.102 P0 Task 3) -------- */

describe('Components page — Badge soft/solid matrix', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/components');
  });

  it('renders Soft and Solid matrix headings under Badge', () => {
    expect(screen.getByRole('heading', { name: 'Badge' })).toBeInTheDocument();
    expect(screen.getByTestId('badge-fixtures')).toBeInTheDocument();
    expect(screen.getByText('Soft (default)')).toBeInTheDocument();
    expect(screen.getByText('Solid')).toBeInTheDocument();
  });

  it('renders six solid samples with solid tone classes', () => {
    const variants = ['neutral', 'running', 'queued', 'warning', 'error', 'preset'] as const;
    for (const variant of variants) {
      const badge = screen.getByTestId(`badge-solid-${variant}`);
      expect(badge).toHaveClass('border-transparent');
      expect(badge).toHaveClass('text-white');
    }
    expect(screen.getByTestId('badge-solid-running')).toHaveClass('bg-green-700');
    expect(screen.getByTestId('badge-solid-running')).toHaveClass('dark:text-brand-deep-blue');
  });
});

describe('Components page — Domain badge matrices', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/components');
  });

  it('renders the Domain Badges section heading and fixture root', () => {
    expect(
      screen.getByRole('heading', { name: 'Domain Badges' }),
    ).toBeInTheDocument();
    expect(screen.getByTestId('domain-badge-fixtures')).toBeInTheDocument();
  });

  it('renders Status and Chapter matrices using Badge variants', () => {
    const statusVariants = ['running', 'queued', 'warning', 'error', 'unknown'] as const;
    for (const value of statusVariants) {
      expect(
        screen.getByTestId(`domain-badge-status-${value}`),
      ).toBeInTheDocument();
    }

    const chapterValues = ['not_started', 'outlined', 'draft', 'finalized', 'published'] as const;
    for (const value of chapterValues) {
      expect(
        screen.getByTestId(`domain-badge-chapter-${value}`),
      ).toBeInTheDocument();
    }
  });

  it('renders Finding and TaskKind matrices with custom color classes', () => {
    const findings = ['open', 'triaged', 'in_review', 'resolved', 'wont_fix', 'duplicate'] as const;
    for (const value of findings) {
      expect(
        screen.getByTestId(`domain-badge-finding-${value}`),
      ).toBeInTheDocument();
    }

    const kinds = ['brainstorm', 'outline', 'chapter', 'research', 'unknown'] as const;
    for (const value of kinds) {
      expect(
        screen.getByTestId(`domain-badge-task-kind-${value}`),
      ).toBeInTheDocument();
    }
  });

  it('uses finding-status-* / memory-task-kind-* token classes (no color-mix arbitraries)', () => {
    // Token segment uses dashes (in_review → in-review, wont_fix → wont-fix).
    const findingCases = [
      ['open', 'open'],
      ['triaged', 'triaged'],
      ['in_review', 'in-review'],
      ['resolved', 'resolved'],
      ['wont_fix', 'wont-fix'],
      ['duplicate', 'duplicate'],
    ] as const;
    for (const [value, token] of findingCases) {
      const badge = screen.getByTestId(`domain-badge-finding-${value}`);
      expect(badge.className).toContain(`bg-finding-status-${token}-bg`);
      expect(badge.className).toContain(`text-finding-status-${token}-text`);
      expect(badge.className).toContain(`border-finding-status-${token}-border`);
      expect(badge.className).not.toContain('color-mix');
    }

    const kinds = ['brainstorm', 'outline', 'chapter', 'research', 'unknown'] as const;
    for (const kind of kinds) {
      const badge = screen.getByTestId(`domain-badge-task-kind-${kind}`);
      expect(badge.className).toContain(`bg-memory-task-kind-${kind}-bg`);
      expect(badge.className).toContain(`text-memory-task-kind-${kind}-text`);
      expect(badge.className).toContain(`border-memory-task-kind-${kind}-border`);
      expect(badge.className).not.toContain('color-mix');
    }
  });
});

/* ---- components page — Select fixtures (V1.101 P2 Task 2) --------------- */

describe('Components page — Select fixtures', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/components');
  });

  it('renders the Select section and fixture root', () => {
    expect(screen.getByRole('heading', { name: 'Select' })).toBeInTheDocument();
    expect(screen.getByTestId('select-fixtures')).toBeInTheDocument();
  });

  it('renders closed select with label association', () => {
    const select = screen.getByTestId('select-fixture-closed');
    expect(select.tagName).toBe('SELECT');
    expect(select).toHaveAttribute('id', 'studio-select-closed');
    expect(select).not.toBeDisabled();
    expect(select).not.toHaveAttribute('aria-invalid');

    const label = screen.getByText('Work profile');
    expect(label.tagName).toBe('LABEL');
    expect(label).toHaveAttribute('for', 'studio-select-closed');
  });

  it('renders disabled select', () => {
    const select = screen.getByTestId('select-fixture-disabled');
    expect(select).toBeDisabled();
  });

  it('renders invalid select with aria-invalid and app-owned error', () => {
    const select = screen.getByTestId('select-fixture-invalid');
    expect(select).toHaveAttribute('aria-invalid', 'true');
    expect(select).toHaveAttribute('aria-describedby', 'studio-select-invalid-helper');
    expect(select.className).toMatch(/border-red-700/);

    const error = document.getElementById('studio-select-invalid-helper');
    expect(error).toBeInTheDocument();
    expect(error).toHaveAttribute('role', 'alert');
    expect(error).toHaveTextContent('Choose a valid executor.');
  });

  it('documents focus-visible class path on the focus fixture', () => {
    const select = screen.getByTestId('select-fixture-focus');
    expect(select.className).toMatch(/focus-visible:border-blue-700/);
    expect(select).toHaveAttribute('id', 'studio-select-focus');
  });

  it('provides a manual open-acceptance fixture without package open API', () => {
    const select = screen.getByTestId('select-fixture-open-manual');
    expect(select.tagName).toBe('SELECT');
    expect(select).not.toHaveAttribute('aria-expanded');
    expect(select).not.toHaveAttribute('open');
  });
});

/* ---- surfaces page — Canvas shell + context menu chrome (V1.108 P1 T4) -- */

describe('Surfaces page — Canvas surfaces fixtures', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/surfaces/canvas');
  });

  it('renders the Canvas section heading', () => {
    expect(
      screen.getByRole('heading', {
        name: 'Canvas — Three mirrored surfaces + shared chrome',
      }),
    ).toBeInTheDocument();
  });

  it('renders the canvas shell chrome fixture with controls + minimap', () => {
    const shell = screen.getByTestId('canvas-shell-chrome');
    expect(shell).toBeInTheDocument();

    const controls = within(shell).getByTestId('canvas-shell-controls');
    expect(
      within(controls).getByRole('button', { name: 'Zoom in' }),
    ).toBeInTheDocument();
    expect(
      within(controls).getByRole('button', { name: 'Zoom out' }),
    ).toBeInTheDocument();
    expect(
      within(controls).getByRole('button', { name: 'Fit view' }),
    ).toBeInTheDocument();

    expect(
      within(shell).getByTestId('canvas-shell-minimap'),
    ).toBeInTheDocument();
  });

  it('renders outline node samples aligned with P0 (Volume / Chapter / Timeline)', () => {
    const matrix = screen.getByTestId('canvas-node-matrix');
    // Volume node
    expect(within(matrix).getByText('Volume II — Journeys')).toBeInTheDocument();

    // Chapter status badges mirror the P0 STATUS_LABEL set.
    const statusBadges = within(matrix).getAllByText('Not started');
    expect(statusBadges.length).toBeGreaterThanOrEqual(1);
    expect(within(matrix).getAllByText('Draft').length).toBeGreaterThanOrEqual(1);
    expect(within(matrix).getAllByText('Finalized').length).toBeGreaterThanOrEqual(1);

    // Timeline event node (unattached)
    expect(within(matrix).getByText('Unattached event')).toBeInTheDocument();
  });

  it('marks the selected node with the canvas-node-border-selected class', () => {
    const matrix = screen.getByTestId('canvas-node-matrix');
    // The finalized-and-selected chapter node title is unique in the matrix.
    const selectedTitle = within(matrix).getByText('Chapter 6 — Descent');
    const nodeShell = selectedTitle.closest('[class*="rounded-card"]');
    expect(nodeShell).not.toBeNull();
    expect(nodeShell!.className).toContain('border-canvas-node-border-selected');
  });

  it('renders context menu chrome matrices with role=menu and Title Case items', () => {
    const matrix = screen.getByTestId('canvas-context-menu-matrix');
    const menus = within(matrix).getAllByRole('menu');
    expect(menus.length).toBeGreaterThanOrEqual(3);

    // Entity (World KB) + Canvas future both have Connect to… → use getAllByText.
    expect(within(matrix).getAllByText('Connect to…').length).toBeGreaterThanOrEqual(1);

    // Path (browser + desktop) both have Copy Path.
    expect(within(matrix).getAllByText('Copy Path').length).toBeGreaterThanOrEqual(1);

    // Path (desktop) — Open With… + Reveal in Finder
    expect(within(matrix).getByText('Open With…')).toBeInTheDocument();
    expect(within(matrix).getByText('Reveal in Finder')).toBeInTheDocument();

    // Canvas (future) — Add Chapter
    expect(within(matrix).getByText('Add Chapter')).toBeInTheDocument();
  });

  it('marks each context menu item with role=menuitem', () => {
    const matrix = screen.getByTestId('canvas-context-menu-matrix');
    const items = within(matrix).getAllByRole('menuitem');
    expect(items.length).toBeGreaterThanOrEqual(4);
  });

  it('does not render light/dark as separate DOM trees (theme toggle drives both)', () => {
    // The fixture renders once; the global ThemeToggle applies .dark to <html>.
    // There should be exactly one canvas shell chrome instance.
    expect(screen.getAllByTestId('canvas-shell-chrome')).toHaveLength(1);
  });

  /* ---- Strategy surface chrome (V1.111 P2 T1) --------------------- */

  it('renders the Strategy surface chrome fixture with shell + inspector + validation', () => {
    const shell = screen.getByTestId('strategy-shell-chrome');
    expect(shell).toBeInTheDocument();

    // Inspector aside mirrors strategy-canvas/inspector-panel ReadOnlyDetails.
    expect(
      within(shell).getByTestId('strategy-inspector-chrome'),
    ).toBeInTheDocument();

    // Validation panel mirrors strategy-canvas/state-machine ValidationPanel.
    expect(
      within(shell).getByTestId('strategy-validation-chrome'),
    ).toBeInTheDocument();
  });

  it('mirrors strategy node kinds (state / join / terminal) with status + kind tags', () => {
    const shell = screen.getByTestId('strategy-shell-chrome');

    // "Drafting" appears both as the state-node header (span[title]) and as the
    // inspector aside heading (h3, read-only mirror of the selected node). The
    // accent stripe lives on the state-node shell — target the span[title] copy.
    const stateHeading = shell.querySelector<HTMLSpanElement>(
      'span[title="Drafting"]',
    );
    expect(stateHeading).not.toBeNull();
    const stateShell = stateHeading!.closest('[class*="border-l-canvas-strategy-accent"]');
    expect(stateShell).not.toBeNull();

    // stateKind mono tag — mirrors StrategyStateNode KindTag.
    expect(within(shell).getAllByText('standard').length).toBeGreaterThanOrEqual(1);

    // Status overlay uses semantic colors — Drafting is the current node.
    expect(within(shell).getByText('Current')).toBeInTheDocument();

    // Join node carries its converge-strategy chip.
    expect(within(shell).getByText('Join · wait_for_all')).toBeInTheDocument();

    // Terminal node shows the End marker.
    expect(within(shell).getByText('End')).toBeInTheDocument();
  });

  it('renders labeled transition edges as static connectors (canvas-strategy-accent)', () => {
    const shell = screen.getByTestId('strategy-shell-chrome');
    const edges = within(shell).getAllByTestId('strategy-edge-sample');
    expect(edges.length).toBeGreaterThanOrEqual(2);

    // Edge labels mirror the RF `label: condition` on strategy-edge.
    expect(within(shell).getByText('draft_ready')).toBeInTheDocument();
    expect(within(shell).getByText('all_done')).toBeInTheDocument();
  });

  /* ---- World KB surface chrome (V1.111 P2 T2) ---------------------- */

  it('renders the World KB surface chrome fixture with shell + inspector', () => {
    const shell = screen.getByTestId('worldkb-shell-chrome');
    expect(shell).toBeInTheDocument();

    // Relationship inspector aside mirrors relationship-inspector.tsx.
    expect(
      within(shell).getByTestId('worldkb-inspector-chrome'),
    ).toBeInTheDocument();
  });

  it('mirrors entity node cards with lifecycle badges for all four states', () => {
    const shell = screen.getByTestId('worldkb-shell-chrome');

    // Confirmed (selected) entity — mirrors WorldKbEntityNode selected paint.
    const confirmedTitle = shell.querySelector<HTMLSpanElement>(
      'span[title="Kael Veynor"]',
    );
    expect(confirmedTitle).not.toBeNull();
    const confirmedShell = confirmedTitle!.closest(
      '[class*="border-canvas-worldkb-entity-card-stroke-selected"]',
    );
    expect(confirmedShell).not.toBeNull();

    // All four lifecycle badge labels render (promotion-* tokens).
    expect(within(shell).getAllByText('Confirmed').length).toBeGreaterThanOrEqual(1);
    expect(within(shell).getAllByText('Merged').length).toBeGreaterThanOrEqual(1);
    expect(within(shell).getAllByText('Pending').length).toBeGreaterThanOrEqual(1);
    expect(within(shell).getAllByText('Rejected').length).toBeGreaterThanOrEqual(1);
  });

  it('mirrors the computable chip on computable BlockType entities', () => {
    const shell = screen.getByTestId('worldkb-shell-chrome');
    // "Act" is a computable block kind → Computable chip renders.
    expect(within(shell).getAllByText('Computable').length).toBeGreaterThanOrEqual(1);
    // The Act entity shows its BlockType tag too.
    expect(within(shell).getAllByText('Act').length).toBeGreaterThanOrEqual(1);
  });

  it('mirrors source-anchor provenance nodes + read-only provenance edges', () => {
    const shell = screen.getByTestId('worldkb-shell-chrome');

    // Source-anchor node — mirrors WorldKbSourceAnchorNode (sourceType + reference).
    expect(within(shell).getAllByText('manuscript').length).toBeGreaterThanOrEqual(1);

    // Source-anchor provenance edge — solid connector in source-anchor-edge token.
    expect(
      within(shell).getAllByTestId('worldkb-source-anchor-edge-sample').length,
    ).toBeGreaterThanOrEqual(1);
  });

  it('mirrors typed relationship edges with confidence bands + suggested-dashed', () => {
    const shell = screen.getByTestId('worldkb-shell-chrome');
    const edges = within(shell).getAllByTestId('worldkb-relationship-edge-sample');
    expect(edges.length).toBeGreaterThanOrEqual(3);

    // Relationship kind labels mirror RELATIONSHIP_KIND_LABELS + custom label.
    expect(within(shell).getAllByText('Allied With').length).toBeGreaterThanOrEqual(1);
    expect(within(shell).getAllByText('Rival Of · suggested').length).toBeGreaterThanOrEqual(1);
    expect(within(shell).getAllByText('Sworn Enemy').length).toBeGreaterThanOrEqual(1);

    // Confidence band labels mirror CONFIDENCE_BAND_LABEL (low / mid / high).
    expect(within(shell).getAllByText('High').length).toBeGreaterThanOrEqual(1);
    expect(within(shell).getAllByText('Medium').length).toBeGreaterThanOrEqual(1);
    expect(within(shell).getAllByText('Low').length).toBeGreaterThanOrEqual(1);
  });

  it('mirrors the relationship inspector with grounded-badge and confidence', () => {
    const inspector = screen.getByTestId('worldkb-inspector-chrome');
    expect(inspector).toBeInTheDocument();
    // Grounded badge mirrors the relationship-grounded-badge token.
    expect(within(inspector).getByText('Grounded')).toBeInTheDocument();
    // Confidence value renders numerically (formatConfidence).
    expect(within(inspector).getByText('0.82')).toBeInTheDocument();
  });
});

/* ---- components page — v0.4 states matrix (V1.121 P1 T4) --------------- */

describe('Components page — Card v0.4 matrix (interactive + title voice)', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/components');
  });

  it('renders rest and interactive cards as real components', () => {
    expect(screen.getByTestId('card-rest')).toBeInTheDocument();
    expect(screen.getByTestId('card-interactive')).toBeInTheDocument();
  });

  it('interactive card carries the v0.4 hover-lift recipe classes', () => {
    const card = screen.getByTestId('card-interactive');
    expect(card.className).toContain('hover:shadow-elevation-2');
    expect(card.className).toContain('motion-safe:hover:-translate-y-px');
    expect(card.className).toContain('duration-popover');
    expect(card.className).toContain('motion-reduce:transition-none');
  });

  it('rest card keeps the static elevation-1 treatment without the recipe', () => {
    const card = screen.getByTestId('card-rest');
    expect(card.className).toContain('shadow-card');
    expect(card.className).not.toContain('hover:shadow-elevation-2');
  });

  it('CardTitle voice="content" swaps to the serif display tier', () => {
    const title = screen.getByTestId('card-title-content');
    expect(title.className).toContain('font-display');
    expect(title.className).toContain('text-display-20');
    expect(title.className).toContain('tracking-tight');
  });

  it('default CardTitle keeps the interface sans treatment', () => {
    const title = screen.getByTestId('card-title-interface');
    expect(title.className).toContain('text-heading-16');
    expect(title.className).toContain('font-heading');
    expect(title.className).not.toContain('font-display');
  });
});

describe('Components page — States v0.4 (error surface + serif empty headline)', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/components');
  });

  it('renders Spinner and LoadingState as the loading cells', () => {
    expect(screen.getByTestId('states-spinner')).toBeInTheDocument();
    expect(screen.getByTestId('states-loading')).toHaveTextContent('Loading data…');
  });

  it('EmptyState headline uses the serif display tier (content voice)', () => {
    const headline = screen
      .getByTestId('states-empty')
      .querySelector('.font-display');
    expect(headline).not.toBeNull();
    expect(headline!.className).toContain('text-display-24');
    expect(headline!).toHaveTextContent('No works yet');
  });

  it('ErrorState uses the error-surface tokens with role="alert"', () => {
    const alert = within(screen.getByTestId('states-error')).getByRole('alert');
    expect(alert.className).toContain('bg-error-surface');
    expect(alert.className).toContain('border-error-surface-border');
  });

  it('ErrorState retry action is live (fires the callback)', () => {
    const retry = within(screen.getByTestId('states-error')).getByRole('button', {
      name: 'Try again',
    });
    expect(
      screen.queryByTestId('states-error-retry-count'),
    ).not.toBeInTheDocument();
    fireEvent.click(retry);
    expect(screen.getByTestId('states-error-retry-count')).toHaveTextContent(
      'Retry requested 1 time.',
    );
  });
});

describe('Components page — Dialog scrim convergence (V1.121)', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/components');
  });

  it('opens with a bg-scrim overlay and an elevation-4 panel', () => {
    fireEvent.click(screen.getByRole('button', { name: 'Open dialog' }));
    const dialog = screen.getByRole('dialog');
    expect(dialog.className).toContain('shadow-elevation-4');
    expect(document.querySelector('.bg-scrim')).not.toBeNull();
    // Close so Radix aria-hidden does not leak into later tests in this suite.
    fireEvent.click(within(dialog).getByRole('button', { name: 'Close dialog' }));
  });
});

describe('Components page — serif discipline (AC-P1-5)', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/components');
  });

  it('confines the serif display voice to content-voice opt-ins', () => {
    // Only the CardTitle voice="content" fixture and the EmptyState headline
    // may carry font-display on this page — interface components (Button,
    // Badge, Input, Select, Tabs, Table) stay sans per DESIGN.md §Design
    // Concept.
    const allowedContainers = ['card-title-content', 'states-empty'];
    const serifEls = Array.from(document.querySelectorAll('.font-display'));
    expect(serifEls.length).toBeGreaterThan(0);
    for (const el of serifEls) {
      const inAllowed = allowedContainers.some(
        (testid) =>
          el.getAttribute('data-testid') === testid ||
          el.closest(`[data-testid="${testid}"]`) !== null,
      );
      expect(inAllowed).toBe(true);
    }
  });
});

describe('Components page — theme toggle coverage (light/dark)', () => {
  it('renders a single DOM tree driven by the .dark class toggle', () => {
    mockMatchMedia(false);
    renderStudio('/components');

    // Light: the states matrix renders once (no per-theme DOM duplication).
    expect(screen.getAllByTestId('card-interactive')).toHaveLength(1);
    expect(document.documentElement.classList.contains('dark')).toBe(false);

    // Toggle to dark — same tree; the token swap is class-driven.
    act(() => screen.getByLabelText(/Switch to dark theme/).click());
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(screen.getAllByTestId('card-interactive')).toHaveLength(1);
    expect(screen.getByTestId('states-error')).toBeInTheDocument();
    expect(screen.getByTestId('dialog-fixtures')).toBeInTheDocument();
  });
});

/* ---- V1.121 P3 T4 — canvas surfaces v0.4 fixture updates ----------------- */

describe('Surfaces page — Canvas surfaces v0.4 fixtures', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/surfaces/canvas');
  });

  it('paints the outline accent spine on outline surface nodes (amber-700)', () => {
    // Outline samples (Volume / Chapter / Scene / Beat) route the accent
    // prop through NodeChromeShell — the spine class targets the amber-700
    // canvas-outline-accent token.
    const matrix = screen.getByTestId('canvas-node-matrix');
    const volumeTitle = within(matrix).getByText('Volume II — Journeys');
    const volumeShell = volumeTitle.closest('[class*="border-l-canvas-outline-accent"]');
    expect(volumeShell).not.toBeNull();
  });

  it('paints the strategy accent spine on every strategy surface node', () => {
    const shell = screen.getByTestId('strategy-shell-chrome');
    // All four strategy node kinds (state, join, terminal, plus the
    // selected drafting state) carry the strategy spine class.
    const spines = shell.querySelectorAll('[class*="border-l-canvas-strategy-accent"]');
    expect(spines.length).toBeGreaterThanOrEqual(3);
  });

  it('paints the worldkb accent spine on every World KB surface node', () => {
    const shell = screen.getByTestId('worldkb-shell-chrome');
    // Entity cards + source-anchor nodes both carry the worldkb spine.
    const spines = shell.querySelectorAll('[class*="border-l-canvas-worldkb-accent"]');
    expect(spines.length).toBeGreaterThanOrEqual(2);
  });

  it('uses min-w-canvas-node-outline-scene-beat for scene + beat samples', () => {
    const matrix = screen.getByTestId('canvas-node-matrix');
    const sceneShells = matrix.querySelectorAll('.min-w-canvas-node-outline-scene-beat');
    // Scene + Beat + Scene (no status) + Beat (selected) = 4 chips.
    expect(sceneShells.length).toBeGreaterThanOrEqual(4);
  });

  it('uses min-w-canvas-node-strategy-secondary for the strategy terminal', () => {
    const shell = screen.getByTestId('strategy-shell-chrome');
    const terminal = shell.querySelector('.min-w-canvas-node-strategy-secondary');
    expect(terminal).not.toBeNull();
  });

  it('mirrors v0.4 elevation + accent on World KB entity + source-anchor nodes', () => {
    const shell = screen.getByTestId('worldkb-shell-chrome');
    // Entity card carries the v0.4 elevation recipe (rest + hover + dragging)
    // via the transition-shadow + hover:shadow-elevation-2 +
    // data-[dragging=true]:shadow-elevation-4 class chain.
    const entityShells = shell.querySelectorAll(
      '[class*="hover:shadow-elevation-2"][class*="data-[dragging=true]:shadow-elevation-4"]',
    );
    expect(entityShells.length).toBeGreaterThanOrEqual(1);
  });

  it('renders the v0.4 elevation state matrix fixture', () => {
    const matrix = screen.getByTestId('canvas-node-v2-elevation-matrix');
    expect(matrix).toBeInTheDocument();
    // Four pinned chips: rest / selected / dragging / selected+dragging.
    expect(within(matrix).getByText('Rest')).toBeInTheDocument();
    expect(within(matrix).getByText('Selected')).toBeInTheDocument();
    expect(within(matrix).getByText('Dragging')).toBeInTheDocument();
    expect(within(matrix).getByText('Selected + dragging')).toBeInTheDocument();
  });

  it('renders the per-surface accent spine matrix fixture', () => {
    const matrix = screen.getByTestId('canvas-node-v2-accents-matrix');
    expect(matrix).toBeInTheDocument();
    // Three accent spines side-by-side. The label appears twice per chip
    // (row header + chip title) — assert on the chip title font-heading span.
    expect(matrix.querySelectorAll('.font-heading.text-copy-14').length).toBe(3);

    // Each spine class is unique per chip.
    expect(
      matrix.querySelectorAll('[class*="border-l-canvas-strategy-accent"]').length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      matrix.querySelectorAll('[class*="border-l-canvas-outline-accent"]').length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      matrix.querySelectorAll('[class*="border-l-canvas-worldkb-accent"]').length,
    ).toBeGreaterThanOrEqual(1);
  });

  it('renders the node-width utility matrix fixture with all five slots', () => {
    const matrix = screen.getByTestId('canvas-node-v2-widths-matrix');
    expect(matrix).toBeInTheDocument();
    // All five min-w-canvas-node-* utility chips are present.
    for (const slot of [
      'strategy-root',
      'strategy-primary',
      'strategy-secondary',
      'outline-scene-beat',
      'default',
    ]) {
      expect(
        within(matrix).getByTestId(`canvas-node-width-${slot}`),
      ).toBeInTheDocument();
    }
    // Each chip uses the named utility class.
    expect(
      matrix.querySelector('.min-w-canvas-node-strategy-root'),
    ).not.toBeNull();
    expect(
      matrix.querySelector('.min-w-canvas-node-default'),
    ).not.toBeNull();
  });
});

/* ---- V1.121 P3 T4 — Tokens page canvas section -------------------------- */

describe('Tokens page — Canvas token gallery (V1.121 P3)', () => {
  beforeEach(() => {
    mockMatchMediaFull();
    renderStudio('/tokens');
  });

  it('renders the Canvas section heading + sub-nav link', () => {
    expect(screen.getByRole('heading', { name: 'Canvas' })).toBeInTheDocument();
    const subnav = screen.getByRole('navigation', { name: 'Token sub-sections' });
    expect(within(subnav).getByRole('link', { name: 'Canvas' })).toHaveAttribute(
      'href',
      '#tokens-canvas',
    );
  });

  it('renders every canvas token group (ambient / node chrome / edges / accents + V1.124 groups)', () => {
    for (const idx of ['0', '1', '2', '3', '4', '5', '6', '7']) {
      expect(
        screen.getByTestId(`canvas-token-group-${idx}`),
      ).toBeInTheDocument();
    }
  });

  it('renders ambient group with dot-grid pattern swatch', () => {
    expect(screen.getByTestId('canvas-ambient-grid-swatch')).toBeInTheDocument();
  });

  it('renders all three accent spine tokens (strategy / outline / worldkb)', () => {
    const accentsGroup = screen.getByTestId('canvas-token-group-3');
    expect(
      within(accentsGroup).getByText('canvas-strategy-accent'),
    ).toBeInTheDocument();
    expect(
      within(accentsGroup).getByText('canvas-outline-accent'),
    ).toBeInTheDocument();
    expect(
      within(accentsGroup).getByText('canvas-worldkb-accent'),
    ).toBeInTheDocument();
  });

  it('renders node chrome tokens including the selected border token', () => {
    const nodeChromeGroup = screen.getByTestId('canvas-token-group-1');
    expect(
      within(nodeChromeGroup).getByText('canvas-node-fill'),
    ).toBeInTheDocument();
    expect(
      within(nodeChromeGroup).getByText('canvas-node-border-selected'),
    ).toBeInTheDocument();
  });

  it('renders the node-width utility gallery with all five slots', () => {
    const widthsGroup = screen.getByTestId('canvas-token-group-widths');
    expect(widthsGroup).toBeInTheDocument();
    for (const slot of [
      'strategy-root',
      'strategy-primary',
      'strategy-secondary',
      'outline-scene-beat',
      'default',
    ]) {
      expect(
        within(widthsGroup).getByTestId(`canvas-node-width-swatch-${slot}`),
      ).toBeInTheDocument();
    }
  });

  it('each accent spine swatch uses border-l-[3px] shape mirroring NodeChromeShell', () => {
    const accentsGroup = screen.getByTestId('canvas-token-group-3');
    const swatches = accentsGroup.querySelectorAll('[style*="border-left: 3px"]');
    // All three spine swatches use the spine shape.
    expect(swatches.length).toBe(3);
  });

  /* ---- V1.124 P1 — Timeline / Layer / Outline pins / Soul Viz axes -------- */

  it('renders Canvas — Timeline accent spine token', () => {
    const group = screen.getByTestId('canvas-token-group-4');
    expect(within(group).getByText('Canvas — Timeline accent spine')).toBeInTheDocument();
    expect(within(group).getByText('canvas-timeline-accent')).toBeInTheDocument();
    const spineSwatches = group.querySelectorAll('[style*="border-left: 3px"]');
    expect(spineSwatches.length).toBe(1);
  });

  it('renders Canvas — Layer accents tokens (brief / narrative / moment)', () => {
    const group = screen.getByTestId('canvas-token-group-5');
    expect(within(group).getByText('Canvas — Layer accents')).toBeInTheDocument();
    expect(within(group).getByText('canvas-layer-brief-accent')).toBeInTheDocument();
    expect(within(group).getByText('canvas-layer-narrative-accent')).toBeInTheDocument();
    expect(within(group).getByText('canvas-layer-moment-accent')).toBeInTheDocument();
  });

  it('renders Canvas — Outline Timeline pins tokens', () => {
    const group = screen.getByTestId('canvas-token-group-6');
    expect(within(group).getByText('Canvas — Outline Timeline pins')).toBeInTheDocument();
    expect(
      within(group).getByText('canvas-outline-timeline-event-pin'),
    ).toBeInTheDocument();
    expect(
      within(group).getByText('canvas-outline-timeline-marker'),
    ).toBeInTheDocument();
  });

  it('renders Soul Viz — Timeline axes tokens', () => {
    const group = screen.getByTestId('canvas-token-group-7');
    expect(within(group).getByText('Soul Viz — Timeline axes')).toBeInTheDocument();
    expect(within(group).getByText('soul-viz-timeline-axis-line')).toBeInTheDocument();
    expect(within(group).getByText('soul-viz-timeline-axis-tick')).toBeInTheDocument();
    expect(within(group).getByText('soul-viz-timeline-axis-label')).toBeInTheDocument();
  });

  it('keeps V1.124 gallery tokens in the DOM across light + dark theme toggle', () => {
    const labels = [
      'canvas-timeline-accent',
      'canvas-layer-brief-accent',
      'canvas-layer-narrative-accent',
      'canvas-layer-moment-accent',
      'canvas-outline-timeline-event-pin',
      'canvas-outline-timeline-marker',
      'soul-viz-timeline-axis-line',
      'soul-viz-timeline-axis-tick',
      'soul-viz-timeline-axis-label',
    ];

    for (const label of labels) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }

    expect(document.documentElement.classList.contains('dark')).toBe(false);
    act(() => screen.getByLabelText(/Switch to dark theme/).click());
    expect(document.documentElement.classList.contains('dark')).toBe(true);

    for (const label of labels) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
  });

  it('V1.124 P1 - CSS-variable bindings: swatch styles reference the expected var(--color-*) names', () => {
    // Verify that the rendered swatch elements have style attributes referencing
    // the correct CSS custom properties. jsdom cannot resolve the actual color
    // values, but asserting the var() binding is present confirms the wiring
    // between tokens.tsx entries and the CSS variable names in tokens.css.
    const groups = [
      { testId: 'canvas-token-group-4', vars: ['var(--color-canvas-timeline-accent)'] },
      {
        testId: 'canvas-token-group-5',
        vars: [
          'var(--color-canvas-layer-brief-accent)',
          'var(--color-canvas-layer-narrative-accent)',
          'var(--color-canvas-layer-moment-accent)',
        ],
      },
      {
        testId: 'canvas-token-group-6',
        vars: [
          'var(--color-canvas-outline-timeline-event-pin)',
          'var(--color-canvas-outline-timeline-marker)',
        ],
      },
      {
        testId: 'canvas-token-group-7',
        vars: [
          'var(--color-soul-viz-timeline-axis-line)',
          'var(--color-soul-viz-timeline-axis-tick)',
          'var(--color-soul-viz-timeline-axis-label)',
        ],
      },
    ];

    for (const { testId, vars } of groups) {
      const group = screen.getByTestId(testId);
      const groupHTML = group.innerHTML;
      for (const v of vars) {
        expect(groupHTML).toContain(v);
      }
    }
  });
});

/* ---- V1.121 P3 T4 — parity sweep (light/dark DOM assertions) ------------ */

describe('V1.121 P3 T4 — parity sweep across all surfaces', () => {
  it('renders all three canvas surfaces + canvas token gallery in a single tree', () => {
    mockMatchMedia(false);
    renderStudio('/surfaces/canvas');

    // Three surface chrome fixtures render in the same DOM tree.
    expect(screen.getByTestId('canvas-shell-chrome')).toBeInTheDocument();
    expect(screen.getByTestId('strategy-shell-chrome')).toBeInTheDocument();
    expect(screen.getByTestId('worldkb-shell-chrome')).toBeInTheDocument();

    // The shared token surface is the same — no per-surface token duplication.
    expect(screen.getAllByTestId('canvas-shell-chrome')).toHaveLength(1);
  });

  it('keeps a single DOM tree across light + dark for canvas surfaces', () => {
    mockMatchMedia(false);
    renderStudio('/surfaces/canvas');

    expect(document.documentElement.classList.contains('dark')).toBe(false);
    expect(screen.getAllByTestId('canvas-shell-chrome')).toHaveLength(1);
    expect(screen.getAllByTestId('strategy-shell-chrome')).toHaveLength(1);
    expect(screen.getAllByTestId('worldkb-shell-chrome')).toHaveLength(1);

    // Toggle to dark — same tree, theme swap is class-driven on <html>.
    act(() => screen.getByLabelText(/Switch to dark theme/).click());
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(screen.getAllByTestId('canvas-shell-chrome')).toHaveLength(1);
    expect(screen.getAllByTestId('strategy-shell-chrome')).toHaveLength(1);
    expect(screen.getAllByTestId('worldkb-shell-chrome')).toHaveLength(1);
  });
});
