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
    route: '/surfaces/agent-picker',
    testId: 'surfaces-agent-picker',
    linkLabel: 'AgentPicker',
  },
  { route: '/surfaces/daemon', testId: 'surfaces-daemon', linkLabel: 'Daemon' },
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

  it('covers welcome / daemon / agent / done step matrices', () => {
    expect(screen.getByTestId('wizard-chrome-steps-welcome')).toBeInTheDocument();
    expect(screen.getByTestId('wizard-chrome-steps-daemon')).toBeInTheDocument();
    expect(screen.getByTestId('wizard-chrome-steps-agent')).toBeInTheDocument();
    expect(screen.getByTestId('wizard-chrome-steps-done')).toBeInTheDocument();
  });

  it('maps step statuses for the agent-active matrix', () => {
    const card = screen.getByTestId('wizard-chrome-card-agent');
    expect(card.querySelector('[data-step-id="welcome"]')).toHaveAttribute(
      'data-step-status',
      'complete',
    );
    expect(card.querySelector('[data-step-id="daemon"]')).toHaveAttribute(
      'data-step-status',
      'complete',
    );
    expect(card.querySelector('[data-step-id="agent"]')).toHaveAttribute(
      'data-step-status',
      'active',
    );
    expect(card.querySelector('[data-step-id="done"]')).toHaveAttribute(
      'data-step-status',
      'pending',
    );
  });

  it('shows numbered step circles (1–4) on the welcome fixture', () => {
    const card = screen.getByTestId('wizard-chrome-card-welcome');
    const circles = card.querySelectorAll('[data-step-id] span.rounded-full');
    expect(circles).toHaveLength(4);
    expect(circles[0]).toHaveTextContent('1');
    expect(circles[1]).toHaveTextContent('2');
    expect(circles[2]).toHaveTextContent('3');
    expect(circles[3]).toHaveTextContent('4');
  });

  it('uses a single horizontal Back / Continue CTA row on daemon and agent', () => {
    const daemonCard = screen
      .getByTestId('wizard-chrome-steps-daemon')
      .querySelector('[data-testid="wizard-chrome-card-daemon"]');
    expect(daemonCard).not.toBeNull();
    const daemonCta = daemonCard!.querySelector('[data-testid="wizard-cta-row"]');
    expect(daemonCta).toHaveAttribute('data-layout', 'horizontal-adjacent');
    expect(daemonCta).toHaveClass('flex', 'items-center');
    expect(daemonCta).not.toHaveClass('flex-col');

    const daemonBack = daemonCta?.querySelector('button[aria-label="Back"]');
    expect(daemonBack).toBeInTheDocument();
    expect(daemonBack).not.toHaveTextContent('Back');
    expect(daemonCta?.querySelectorAll('button')[1]).toHaveTextContent('Continue');

    const agentCard = screen.getByTestId('wizard-chrome-card-agent');
    const agentCta = agentCard.querySelector('[data-testid="wizard-cta-row"]');
    expect(agentCta?.querySelector('button[aria-label="Back"]')).toBeInTheDocument();
  });

  it('omits Back on welcome and done', () => {
    const welcomeCard = screen.getByTestId('wizard-chrome-card-welcome');
    expect(
      welcomeCard.querySelector('[data-testid="wizard-cta-row"] button[aria-label="Back"]'),
    ).not.toBeInTheDocument();

    const doneCard = screen.getByTestId('wizard-chrome-card-done');
    expect(
      doneCard.querySelector('[data-testid="wizard-cta-row"] button[aria-label="Back"]'),
    ).not.toBeInTheDocument();
    expect(doneCard.querySelector('[data-testid="wizard-cta-row"]')).toHaveTextContent(
      'Open Nexus',
    );
  });

  it('covers daemon starting / running / error chips', () => {
    expect(screen.getByTestId('daemon-chip-starting')).toHaveTextContent(
      'Starting daemon…',
    );
    expect(screen.getAllByTestId('daemon-chip-running').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('Daemon is running.')).toBeInTheDocument();
    const errorChip = screen.getByTestId('daemon-chip-error');
    expect(errorChip).toHaveTextContent(/taking longer/i);
    expect(within(errorChip).getByRole('button', { name: 'Retry' })).toBeInTheDocument();
    // Retry is first; concise left-aligned small copy below.
    const errorChildren = Array.from(errorChip.children);
    expect(errorChildren[0]?.querySelector('button')).toHaveTextContent('Retry');
    expect(errorChildren[1]?.tagName).toBe('P');
    expect(errorChildren[1]).toHaveClass('text-left', 'text-copy-12');
  });

  it('starts step connectors below each circle (nothing above step 1)', () => {
    const welcomeCard = screen.getByTestId('wizard-chrome-card-welcome');
    const connectors = welcomeCard.querySelectorAll('[data-testid="step-connector"]');
    expect(connectors.length).toBe(3);
    connectors.forEach((el) => {
      expect(el).toHaveStyle({
        top: 'calc(50% + var(--color-setup-wizard-step-circle-size) / 2)',
      });
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

  it('renders section nav with Agent, Connection, Setup (no Workspace)', () => {
    const hostRoot = screen.getByTestId('settings-host-fixtures');
    const sectionNav = within(hostRoot).getByTestId('settings-section-nav');
    expect(
      within(sectionNav).getByTestId('settings-section-nav-agent'),
    ).toHaveTextContent('Agent');
    expect(
      within(sectionNav).getByTestId('settings-section-nav-connection'),
    ).toHaveTextContent('Connection');
    expect(
      within(sectionNav).getByTestId('settings-section-nav-setup'),
    ).toHaveTextContent('Setup');
    expect(
      within(sectionNav).queryByTestId('settings-section-nav-workspace'),
    ).not.toBeInTheDocument();
    expect(within(sectionNav).queryByText('Workspace')).not.toBeInTheDocument();
  });

  it('defaults to Agent section with empty frame and locked shell helper', () => {
    const hostRoot = screen.getByTestId('settings-host-fixtures');
    const shellPages = within(hostRoot).getAllByTestId(
      'settings-shell-page-chrome',
    );
    expect(shellPages.length).toBeGreaterThanOrEqual(1);
    expect(
      within(hostRoot).getByTestId('settings-section-nav-agent'),
    ).toHaveAttribute('aria-current', 'page');
    const outlet = within(hostRoot).getByTestId('settings-shell-outlet');
    expect(
      within(outlet).getByTestId('settings-section-frame-agent'),
    ).toBeInTheDocument();
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

  it('switches empty section frames when section nav is clicked', () => {
    const hostRoot = screen.getByTestId('settings-host-fixtures');
    const outlet = within(hostRoot).getByTestId('settings-shell-outlet');
    const connectionTab = within(hostRoot).getByTestId(
      'settings-section-nav-connection',
    );
    fireEvent.click(connectionTab);
    expect(connectionTab).toHaveAttribute('aria-current', 'page');
    expect(
      within(outlet).getByTestId('settings-section-frame-connection'),
    ).toBeInTheDocument();
    expect(
      within(outlet).queryByTestId('settings-section-frame-agent'),
    ).not.toBeInTheDocument();
  });

  it('renders static empty frames for all three Must sections', () => {
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
