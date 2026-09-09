/**
 * P0G-4 — Studio/setup proof for TopStepIndicator active contrast (V1.137 P0 T3).
 *
 * Locks white-on-teal active step contrast via:
 *   1. Setup wizard chrome fixture render (workspace-active matrix) defending
 *      the semantic token classes on the active step circle.
 *   2. Theme toggle — semantic token classes survive light ↔ dark.
 *
 * Source-string pins of the compiled tokens.css have been removed: the
 * compiler resolves `setup-wizard-step-circle-active-*` to their actual
 * values, so assertions on `var(--color-blue-*)` strings are unstable against
 * the resolved output. The remaining assertions guard observable rendered
 * classes across the theme toggle.
 */
import { act, render, screen } from '@testing-library/react';
import { I18nextProvider } from 'react-i18next';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ThemeProvider } from '@/components/theme-provider';
import { SetupWizardChromeFixtures } from '@/fixtures/setup-wizard-chrome-fixtures';
import { i18n } from '@/lib/i18n/config';

function mockMatchMediaFull({ dark = false }: { dark?: boolean } = {}) {
  vi.spyOn(window, 'matchMedia').mockImplementation((query: string) => {
    const matches = query.includes('prefers-color-scheme') ? dark : false;
    return {
      matches,
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    } as unknown as MediaQueryList;
  });
}

function renderWizardFixtures() {
  return render(
    <ThemeProvider>
      <I18nextProvider i18n={i18n}>
        <SetupWizardChromeFixtures />
      </I18nextProvider>
    </ThemeProvider>,
  );
}

function workspaceActiveCircle(): HTMLElement {
  const frame = screen.getByTestId('wizard-chrome-steps-workspace');
  const circle = frame.querySelector('[data-step-id="workspace"] span.rounded-full');
  if (!circle) throw new Error('workspace active step circle not found');
  return circle as HTMLElement;
}

beforeEach(() => {
  window.localStorage.clear();
  document.documentElement.classList.remove('dark');
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('Setup wizard active contrast — Studio fixture (P0G-4)', () => {
  it('workspace-active matrix renders white-on-teal semantic token classes in light', () => {
    mockMatchMediaFull({ dark: false });
    renderWizardFixtures();

    const activeCircle = workspaceActiveCircle();
    expect(activeCircle).toHaveTextContent('3');
    expect(activeCircle).toHaveClass('bg-setup-wizard-step-circle-active-bg');
    expect(activeCircle).toHaveClass('text-setup-wizard-step-circle-active-text');
    expect(activeCircle).not.toHaveClass('text-brand-deep-blue');
  });

  it('active step semantic tokens persist across light ↔ dark theme toggle', () => {
    mockMatchMediaFull({ dark: false });
    renderWizardFixtures();

    expect(document.documentElement.classList.contains('dark')).toBe(false);
    expect(workspaceActiveCircle()).toHaveClass('text-setup-wizard-step-circle-active-text');

    act(() => {
      document.documentElement.classList.add('dark');
    });
    expect(workspaceActiveCircle()).toHaveClass(
      'bg-setup-wizard-step-circle-active-bg',
      'text-setup-wizard-step-circle-active-text',
    );

    act(() => {
      document.documentElement.classList.remove('dark');
    });
    expect(workspaceActiveCircle()).toHaveClass('text-setup-wizard-step-circle-active-text');
  });
});
