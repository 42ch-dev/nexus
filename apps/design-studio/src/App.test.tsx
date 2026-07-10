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

/* ---- surfaces section menu / deep links (V1.102 P2 Task 1) -------------- */

const SURFACES_SECTION_ROUTES = [
  { route: '/surfaces', testId: 'surfaces-index', linkLabel: 'Overview' },
  { route: '/surfaces/setup', testId: 'surfaces-setup', linkLabel: 'Setup' },
  { route: '/surfaces/shell', testId: 'surfaces-shell', linkLabel: 'Shell' },
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
    expect(screen.getByText('All Works')).toBeInTheDocument();
  });

  it('renders Worlds and Findings nav groups', () => {
    expect(screen.getAllByText('Worlds').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('Findings').length).toBeGreaterThanOrEqual(1);
  });

  it('renders the profile footer with creator name', () => {
    expect(screen.getAllByText('Local Creator').length).toBeGreaterThanOrEqual(1);
  });

  it('renders add-profile button with accessible label', () => {
    expect(
      screen.getByRole('button', { name: 'Add profile' }),
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
      screen.getAllByRole('button', { name: 'Restart Nexus' }).length,
    ).toBeGreaterThanOrEqual(2);
    expect(screen.getByText('Reset local database')).toBeInTheDocument();
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
    expect(screen.getByText('Scanning for local ACP agents…')).toBeInTheDocument();
    expect(screen.getByText('No agents found on PATH')).toBeInTheDocument();
    expect(screen.getByText('Could not scan for agents')).toBeInTheDocument();
  });

  it('shows custom launch on empty and error', () => {
    const customFields = screen.getAllByTestId('agent-picker-custom-launch');
    expect(customFields.length).toBeGreaterThanOrEqual(2);
  });

  it('renders installed and not-installed cards in mixed fixture', () => {
    const installed = screen.getAllByTestId('agent-card-claude-code');
    expect(installed.length).toBeGreaterThanOrEqual(1);
    expect(installed[0]).toHaveAttribute('data-installed', 'true');

    const missing = screen.getAllByTestId('agent-card-gemini-cli');
    expect(missing.length).toBeGreaterThanOrEqual(1);
    expect(missing[0]).toHaveAttribute('data-installed', 'false');
  });

  it('hides outbound links when URLs are missing (Cursor Agent)', () => {
    const hiddenLinksCard = screen.getByTestId('agent-card-cursor-agent');
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
      .getAllByTestId('agent-card-select-claude-code')
      .filter((el) => el.getAttribute('aria-pressed') === 'true');
    expect(pressed.length).toBeGreaterThanOrEqual(1);
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
    expect(screen.getByTestId('settings-shell-chrome')).toBeInTheDocument();
    const link = screen.getByTestId('settings-footer-utility-link');
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
    expect(agentSection).toHaveAttribute('data-preselected', 'codex');
    expect(
      within(outlet).queryByTestId('settings-section-frame-agent'),
    ).not.toBeInTheDocument();
    expect(
      within(hostRoot).getAllByText(
        /Manage your local agent, daemon connection, and setup options/i,
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
    expect(section).toHaveAttribute('data-preselected', 'codex');
    expect(
      within(agentRoot).getByText(
        /Choose which local ACP agent Nexus uses for creative work/i,
      ),
    ).toBeInTheDocument();
    const pressed = within(agentRoot)
      .getAllByTestId('agent-card-select-codex')
      .filter((el) => el.getAttribute('aria-pressed') === 'true');
    expect(pressed.length).toBeGreaterThanOrEqual(1);
    // Preselect is Codex, not the first-installed Claude default.
    const claudePressed = within(agentRoot)
      .getAllByTestId('agent-card-select-claude-code')
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
    ).toHaveTextContent('Trust This Certificate and Connect');
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
    ).toHaveTextContent('Re-run Setup');
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
    ).toHaveTextContent('Re-run Setup');
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
    expect(within(chrome).getByText('Re-run Setup')).toBeInTheDocument();
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
    expect(cta).toHaveTextContent('Change Folder…');
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
    const cards = screen.getAllByTestId('agent-card-claude-code');
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
