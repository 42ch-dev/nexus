/**
 * Outline authoring Studio fixtures — boundary + variant render tests
 * (V1.200, Greptile issue 5 studio-first).
 *
 * Mirrors the timeline-canvas-fixtures test recipe:
 *   1. Presentational boundary — fixture imports no @xyflow/react, no
 *      contracts, no daemon clients, no useTranslation, no Tauri.
 *   2. Authoring card states — empty / with-draft / bound (disabled + live
 *      affordances, chapter badges, empty-state copy).
 *   3. Bind control states — unbound / bound / disabled-when-no-bound-World.
 *   4. Themes — every variant renders a scoped light + `.dark` pair, and the
 *      whole fixture renders without throw under a document-level `.dark`.
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { OutlineAuthoringFixtures } from '@/fixtures/outline-authoring-fixtures';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const FIXTURE_SOURCE_PATH = path.resolve(
  __dirname,
  '../outline-authoring-fixtures.tsx',
);

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
  vi.spyOn(window, 'matchMedia').mockReturnValue(
    media as unknown as MediaQueryList,
  );
}

beforeEach(() => {
  document.documentElement.classList.remove('dark');
});

afterEach(() => {
  vi.restoreAllMocks();
  document.documentElement.classList.remove('dark');
});

// ---------------------------------------------------------------------------
// Boundary — fixture file stays RF / contracts / daemon / i18n free
// ---------------------------------------------------------------------------

describe('outline-authoring-fixtures presentational boundary', () => {
  it('does not import @xyflow/react, contracts, daemon clients, or useTranslation', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    const imports = source.match(/^import .*$/gm) ?? [];
    expect(imports.join('\n')).not.toMatch(/xyflow/);
    expect(imports.join('\n')).not.toMatch(/nexus-contracts/);
    expect(imports.join('\n')).not.toMatch(/useTranslation/);
    expect(imports.join('\n')).not.toMatch(/lib\/nexus/);
    expect(imports.join('\n')).not.toMatch(/NexusClient/);
    expect(imports.join('\n')).not.toMatch(/@tauri-apps/);
  });

  it('imports promoted primitives from @42ch/nexus-ui public API only', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    expect(source).toMatch(/from '@42ch\/nexus-ui';/);
    // Transitional @web-ui/states import carries the inline annotation on the
    // module-path line (AGENTS.md annotation placement rule).
    expect(source).toMatch(/'@web-ui\/states'; .*transitional/);
  });
});

// ---------------------------------------------------------------------------
// Rendering — frames + authoring card states
// ---------------------------------------------------------------------------

describe('OutlineAuthoringFixtures render', () => {
  it('renders both fixture frames and all light/dark pairs', () => {
    mockMatchMedia(false);
    render(<OutlineAuthoringFixtures />);

    expect(screen.getByTestId('outline-authoring-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('outline-authoring-fixture-card')).toBeInTheDocument();
    expect(screen.getByTestId('outline-world-event-fixture')).toBeInTheDocument();

    for (const testId of [
      'outline-authoring-card-empty',
      'outline-authoring-card-draft',
      'outline-authoring-card-bound',
      'outline-world-event-unbound',
      'outline-world-event-bound',
      'outline-world-event-disabled',
    ]) {
      expect(screen.getByTestId(`${testId}-light`)).toBeInTheDocument();
      expect(screen.getByTestId(`${testId}-dark`)).toBeInTheDocument();
    }
  });

  it('empty variant: no-chapters hint, honest empty state, add gated off', () => {
    mockMatchMedia(false);
    render(<OutlineAuthoringFixtures />);

    const light = screen.getByTestId('outline-authoring-card-empty-light');
    expect(
      within(light).getByText('Add a chapter before authoring scenes.'),
    ).toBeInTheDocument();
    expect(within(light).getByText('No scenes yet')).toBeInTheDocument();
    expect(
      within(light).getByText(
        'Scenes you add here appear on the Work Timeline Moment layer.',
      ),
    ).toBeInTheDocument();
    expect(within(light).queryByTestId('outline-scene-list-empty')).toBeNull();
    expect(within(light).getByTestId('outline-scene-chapter-empty')).toBeDisabled();
    expect(within(light).getByTestId('outline-add-scene-empty')).toBeDisabled();
  });

  it('with-draft variant: add scene + add beat enabled for in-progress titles', () => {
    mockMatchMedia(false);
    render(<OutlineAuthoringFixtures />);

    const light = screen.getByTestId('outline-authoring-card-draft-light');
    expect(
      within(light).getByTestId('outline-add-scene-draft'),
    ).not.toBeDisabled();
    expect(
      within(light).getByTestId('outline-scene-title-draft'),
    ).toHaveValue('Arrival at the Ashen Gate');
    const beatInput = within(light).getByTestId('outline-beat-title-sc-draft-1');
    expect(beatInput).toHaveValue('Hook');
    expect(
      within(light).getByTestId('outline-add-beat-sc-draft-1'),
    ).not.toBeDisabled();
    expect(
      within(light).getByTestId('outline-remove-scene-sc-draft-1'),
    ).not.toBeDisabled();
  });

  it('bound variant: scenes chapter-bound with badges and live remove affordances', () => {
    mockMatchMedia(false);
    render(<OutlineAuthoringFixtures />);

    const light = screen.getByTestId('outline-authoring-card-bound-light');
    const list = within(light).getByTestId('outline-scene-list-bound');
    expect(list.children).toHaveLength(2);
    expect(within(light).getAllByText('Ch. 1')).toHaveLength(1);
    expect(within(light).getAllByText('Ch. 2')).toHaveLength(1);
    expect(
      within(light).getByTestId('outline-remove-scene-sc-bound-1'),
    ).not.toBeDisabled();
    expect(
      within(light).getByTestId('outline-remove-beat-bt-bound-3'),
    ).not.toBeDisabled();
    // Blank title drafts keep both add buttons gated off.
    expect(within(light).getByTestId('outline-add-scene-bound')).toBeDisabled();
    expect(
      within(light).getByTestId('outline-add-beat-sc-bound-1'),
    ).toBeDisabled();
  });
});

// ---------------------------------------------------------------------------
// Rendering — World-event bind/unbind control states
// ---------------------------------------------------------------------------

describe('Outline World-event bind control fixtures', () => {
  it('unbound: picker enabled, Bind gated on a selected World event', () => {
    mockMatchMedia(false);
    render(<OutlineAuthoringFixtures />);

    const light = screen.getByTestId('outline-world-event-unbound-light');
    const select = within(light).getByTestId('outline-world-event-select-unbound');
    expect(select).not.toBeDisabled();
    expect(
      within(select).getByText('Bind World event…'),
    ).toBeInTheDocument();
    expect(within(select).getByText('The Fall of Ashen Gate')).toBeInTheDocument();
    expect(within(light).getByTestId('outline-world-event-bind-unbound')).toBeDisabled();
    // Bound row on the same panel carries a live Unbind.
    expect(
      within(light).getByTestId('outline-world-event-unbind-unbound'),
    ).not.toBeDisabled();
    expect(
      within(light).getByText('World event: The Fall of Ashen Gate'),
    ).toBeInTheDocument();
  });

  it('disabled (no bound World): hint copy and every affordance disabled, none hidden', () => {
    mockMatchMedia(false);
    render(<OutlineAuthoringFixtures />);

    const light = screen.getByTestId('outline-world-event-disabled-light');
    expect(
      within(light).getByText('Bind a World to this Work to bind World events.'),
    ).toBeInTheDocument();
    expect(
      within(light).getByTestId('outline-world-event-select-disabled'),
    ).toBeDisabled();
    expect(
      within(light).getByTestId('outline-world-event-bind-disabled'),
    ).toBeDisabled();
    expect(
      within(light).getByTestId('outline-world-event-unbind-disabled'),
    ).toBeDisabled();
  });
});

// ---------------------------------------------------------------------------
// Themes — scoped dark pairs + document-level dark render
// ---------------------------------------------------------------------------

describe('OutlineAuthoringFixtures themes', () => {
  it('renders every variant inside a scoped .dark pair panel', () => {
    mockMatchMedia(false);
    render(<OutlineAuthoringFixtures />);

    const root = screen.getByTestId('outline-authoring-fixtures');
    const darkPanels = root.querySelectorAll('[data-testid$="-dark"].dark');
    expect(darkPanels).toHaveLength(6);
  });

  it('renders without throw under a document-level .dark class', () => {
    mockMatchMedia(true);
    document.documentElement.classList.add('dark');
    expect(() => render(<OutlineAuthoringFixtures />)).not.toThrow();
    expect(screen.getByTestId('outline-authoring-fixtures')).toBeInTheDocument();
    expect(
      screen.getByTestId('outline-authoring-fixture-card'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('outline-world-event-fixture'),
    ).toBeInTheDocument();
  });
});
