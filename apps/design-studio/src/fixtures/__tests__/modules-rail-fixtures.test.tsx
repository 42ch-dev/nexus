import { cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { ModulesRailFixtures } from '@/fixtures/modules-rail-fixtures';
import { ThemeProvider } from '@/components/theme-provider';

/**
 * The `/modules` rail has five states worth eyeballing: the ready list with a
 * selection, the manifest detail beside the no-selection prompt, and the three
 * Run Studio reads (pending / success / deep-linked detail error). Each frame
 * owns one `data-testid`; the assertions below pin the state that makes the
 * frame different from its siblings.
 */
const VARIANTS = [
  'modules-rail-fixture-list',
  'modules-rail-fixture-detail',
  'modules-rail-fixture-run-pending',
  'modules-rail-fixture-run-success',
  'modules-rail-fixture-run-error',
] as const;

afterEach(() => {
  cleanup();
  document.documentElement.classList.remove('dark');
});

describe('ModulesRailFixtures', () => {
  it('mounts every variant frame', () => {
    render(<ModulesRailFixtures />);

    expect(screen.getByTestId('modules-rail-fixtures')).toBeInTheDocument();
    for (const testId of VARIANTS) {
      expect(screen.getByTestId(testId)).toBeInTheDocument();
    }
  });

  it('gives each mounted Run Studio its own control ids', () => {
    render(<ModulesRailFixtures />);
    const runFrames = [
      'modules-rail-fixture-run-pending',
      'modules-rail-fixture-run-success',
      'modules-rail-fixture-run-error',
    ] as const;

    // getByLabelText resolves through htmlFor → id: each label reaches the
    // control rendered beside it, not the first Run Studio in the document.
    // The schema-driven Run form derives its field ids as
    // `${idPrefix}-${name}`; a literal prefix re-duplicated
    // run-form-attacker_id across the three frames (I-000086 occ 97).
    for (const label of [
      'World',
      'Filter by status',
      'Filter by World',
      'Attacker',
      'Defender',
    ]) {
      const ids = runFrames.map((testId) => {
        const frame = screen.getByTestId(testId);
        return within(frame).getByLabelText(label).id;
      });

      expect(new Set(ids).size).toBe(runFrames.length);
      for (const id of ids) {
        expect(id.length).toBeGreaterThan(0);
        expect(document.querySelectorAll(`[id="${id}"]`)).toHaveLength(1);
      }
    }
  });

  it('renders the ready list with one selected module', () => {
    render(<ModulesRailFixtures />);
    const frame = screen.getByTestId('modules-rail-fixture-list');

    expect(within(frame).getByTestId('modules-page-body')).toBeInTheDocument();
    expect(within(frame).getByRole('button', { name: 'Basic Combat', pressed: true })).toBeInTheDocument();
    expect(
      within(frame).getByRole('button', { name: 'Scouting Party', pressed: false }),
    ).toBeInTheDocument();
  });

  it('renders manifest fields beside the select prompt', () => {
    render(<ModulesRailFixtures />);
    const frame = screen.getByTestId('modules-rail-fixture-detail');

    expect(within(frame).getByText('Select a module')).toBeInTheDocument();
    expect(within(frame).getByRole('heading', { name: 'Basic Combat' })).toBeInTheDocument();
    expect(within(frame).getByText('ABI version')).toBeInTheDocument();
    expect(within(frame).getByText('3')).toBeInTheDocument();
    expect(within(frame).getByText('Host functions')).toBeInTheDocument();
    expect(within(frame).getByText('Schemas')).toBeInTheDocument();
  });

  it('renders the pending Run state', () => {
    render(<ModulesRailFixtures />);
    const frame = screen.getByTestId('modules-rail-fixture-run-pending');

    expect(within(frame).getByTestId('run-studio')).toBeInTheDocument();
    expect(within(frame).getByRole('button', { name: 'Running…' })).toBeDisabled();
    expect(within(frame).getByText('This Run is still executing.')).toBeInTheDocument();
  });

  it('renders the proposal inspector for a Run awaiting review', () => {
    render(<ModulesRailFixtures />);
    const frame = screen.getByTestId('modules-rail-fixture-run-success');

    expect(within(frame).getByTestId('run-inspector')).toBeInTheDocument();
    expect(within(frame).getByText('Aria strikes Brann')).toBeInTheDocument();
    expect(within(frame).getByTestId('run-inspector-accept')).toBeEnabled();
    expect(within(frame).getByTestId('run-inspector-discard')).toBeEnabled();
  });

  it('renders the deep-linked Run detail read error', () => {
    render(<ModulesRailFixtures />);
    const frame = screen.getByTestId('modules-rail-fixture-run-error');

    expect(within(frame).getByText('Could not load this Run')).toBeInTheDocument();
    expect(within(frame).queryByTestId('run-inspector')).not.toBeInTheDocument();
  });

  it('renders every variant under both themes', () => {
    for (const forced of ['light', 'dark'] as const) {
      render(
        <ThemeProvider forcedTheme={forced}>
          <ModulesRailFixtures />
        </ThemeProvider>,
      );

      // The Studio theme system is the authority for the marker (`<html>.dark`);
      // asserting it proves the fixture renders under the resolved theme, not
      // just that a fixture-owned class survived.
      expect(document.documentElement.classList.contains('dark')).toBe(forced === 'dark');
      for (const testId of VARIANTS) {
        expect(screen.getByTestId(testId)).toBeInTheDocument();
      }

      cleanup();
    }
  });
});
