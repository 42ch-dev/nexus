/**
 * P0G-4 — Studio/setup proof for TopStepIndicator active contrast (V1.137 P0 T3).
 *
 * Locks white-on-teal in light theme via:
 *   1. tokens.css SSOT pair (active bg → blue-1000, active text → brand-white)
 *   2. Setup wizard chrome fixture render (workspace-active matrix)
 *   3. Theme toggle — semantic token classes survive light ↔ dark
 */
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { act, render, screen } from '@testing-library/react';
import { I18nextProvider } from 'react-i18next';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ThemeProvider } from '@/components/theme-provider';
import { SetupWizardChromeFixtures } from '@/fixtures/setup-wizard-chrome-fixtures';
import { i18n } from '@/lib/i18n/config';

const tokensCss = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), '../../../../../tooling/design-tokens/src/tokens.css'),
  'utf8',
);
const [lightTokensBlock, darkTokensBlock] = tokensCss.split(/\n\.dark \{/);

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

describe('Setup wizard active contrast — tokens SSOT (P0G-4)', () => {
  it('light :root pairs active circle bg blue-1000 with brand-white text', () => {
    expect(lightTokensBlock).toContain(
      '--color-setup-wizard-step-circle-active-bg: var(--color-blue-1000)',
    );
    expect(lightTokensBlock).toContain(
      '--color-setup-wizard-step-circle-active-text: var(--color-brand-white)',
    );
  });

  it('dark .dark keeps active circle blue-700 + brand-deep-blue text (Q2)', () => {
    expect(darkTokensBlock).toContain(
      '--color-setup-wizard-step-circle-active-bg: var(--color-blue-700)',
    );
    expect(darkTokensBlock).toContain(
      '--color-setup-wizard-step-circle-active-text: var(--color-brand-deep-blue)',
    );
  });
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
