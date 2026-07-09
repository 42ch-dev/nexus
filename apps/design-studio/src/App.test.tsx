/**
 * Design Studio smoke tests — T7 + T4 surface-fixture coverage.
 *
 * Coverage:
 *   1. App renders the landing page (HomePage) with section links.
 *   2. Theme toggle switches light ↔ dark and applies the `.dark` class.
 *   3. Each gallery section route renders its heading.
 *   4. Surfaces page renders setup wizard and app shell fixtures with
 *      expected labels and structural elements.
 *
 * These tests catch route/render regressions without snapshot-exhausting every
 * swatch or component variant. Follow apps/web conventions: vitest + jsdom +
 * @testing-library/react.
 */
import { act, render, screen } from '@testing-library/react';
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

/* ---- surfaces page fixtures (T4) ---------------------------------------- */

describe('Surfaces page — setup wizard chrome fixtures', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/surfaces');
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

    const daemonButtons = daemonCta?.querySelectorAll('button');
    expect(daemonButtons?.[0]).toHaveTextContent('Back');
    expect(daemonButtons?.[1]).toHaveTextContent('Continue');

    const agentCard = screen.getByTestId('wizard-chrome-card-agent');
    const agentCta = agentCard.querySelector('[data-testid="wizard-cta-row"]');
    expect(agentCta?.querySelectorAll('button')[0]).toHaveTextContent('Back');
  });

  it('omits Back on welcome and done', () => {
    const welcomeCard = screen.getByTestId('wizard-chrome-card-welcome');
    expect(
      welcomeCard.querySelector('[data-testid="wizard-cta-row"]')?.textContent,
    ).not.toMatch(/Back/);

    const doneCard = screen.getByTestId('wizard-chrome-card-done');
    expect(
      doneCard.querySelector('[data-testid="wizard-cta-row"]')?.textContent,
    ).not.toMatch(/Back/);
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
    expect(screen.getByTestId('daemon-chip-error')).toHaveTextContent(/taking longer/i);
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
  });
});

describe('Surfaces page — app shell fixture', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/surfaces');
  });

  it('renders Creator and Orchestrator tabs', () => {
    expect(screen.getByText('Creator')).toBeInTheDocument();
    expect(screen.getByText('Orchestrator')).toBeInTheDocument();
  });

  it('renders Works nav group and All Works child', () => {
    // "Works" appears as a nav group label — verify at least one instance.
    expect(screen.getAllByText('Works').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('All Works')).toBeInTheDocument();
  });

  it('renders Worlds and Findings nav groups', () => {
    expect(screen.getByText('Worlds')).toBeInTheDocument();
    expect(screen.getByText('Findings')).toBeInTheDocument();
  });

  it('renders the profile footer with creator name', () => {
    expect(screen.getByText('Local Creator')).toBeInTheDocument();
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
  it('renders daemon status heading and healthy badge', () => {
    mockMatchMedia(false);
    renderStudio('/surfaces');

    expect(screen.getByText('Daemon running')).toBeInTheDocument();
    expect(screen.getByText('healthy')).toBeInTheDocument();
  });
});

/* ---- surfaces page — AgentPicker fixtures (V1.101 P0 Task 2) ------------ */

describe('Surfaces page — AgentPicker fixtures', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    renderStudio('/surfaces');
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
