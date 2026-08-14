/**
 * Mental surfacing Studio fixtures — boundary + five-state render tests
 * (V1.164 P3 Task 2, AR-6 studio-first).
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { MentalSurfacingFixtures } from '@/fixtures/mental-surfacing-fixtures';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const FIXTURE_SOURCE_PATH = path.resolve(
  __dirname,
  '../mental-surfacing-fixtures.tsx',
);

/**
 * getByText compares the normalized DOM text against the RAW expected string
 * (only the container side is normalized) — pre-normalize pretty-JSON values
 * the same way the default normalizer collapses whitespace.
 */
const prettyJsonText = (value: unknown) =>
  JSON.stringify(value, null, 2).replace(/\s+/g, ' ').trim();

beforeEach(() => {
  document.documentElement.classList.remove('dark');
});

afterEach(() => {
  document.documentElement.classList.remove('dark');
});

describe('mental-surfacing-fixtures presentational boundary', () => {
  it('does not import @xyflow/react, contracts, daemon clients, or useTranslation', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    const imports = source.match(/^import .*$/gm) ?? [];
    expect(imports.join('\n')).not.toMatch(/xyflow/);
    expect(imports.join('\n')).not.toMatch(/nexus-contracts/);
    expect(imports.join('\n')).not.toMatch(/useTranslation/);
    expect(imports.join('\n')).not.toMatch(/lib\/nexus/);
    expect(imports.join('\n')).not.toMatch(/NexusClient/);
  });

  it('mirrors the Task 1 modules wire shape locally (additive optional modules bag)', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    expect(source).toMatch(/modules\?: Record<string, unknown>/);
    expect(source).toMatch(/modules\?: Record<string, unknown> \| null/);
  });
});

describe('MentalSurfacingFixtures render — five states (light)', () => {
  it('renders all five fixture frames', () => {
    render(<MentalSurfacingFixtures />);
    expect(screen.getByTestId('mental-surfacing-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('mental-fixture-character-populated')).toBeInTheDocument();
    expect(screen.getByTestId('mental-fixture-character-absent')).toBeInTheDocument();
    expect(screen.getByTestId('mental-fixture-event-observers')).toBeInTheDocument();
    expect(screen.getByTestId('mental-fixture-event-empty')).toBeInTheDocument();
    expect(screen.getByTestId('mental-fixture-event-absent')).toBeInTheDocument();
  });

  it('(a) character with populated mental bag shows the Mental State section with beliefs / goals / emotions', () => {
    render(<MentalSurfacingFixtures />);
    const host = screen.getByTestId('mental-character-populated-host');
    expect(within(host).getByText('Mental State')).toBeInTheDocument();
    expect(within(host).getByTestId('mental-state-section')).toBeInTheDocument();
    // AC proof — at minimum beliefs / goals / emotions visible.
    expect(within(host).getByText('Beliefs')).toBeInTheDocument();
    expect(within(host).getByText('Goals')).toBeInTheDocument();
    expect(within(host).getByText('Emotions')).toBeInTheDocument();
    // Structured values render as JSON.
    expect(
      within(host).getByText(prettyJsonText({ ref: 'kb_bo_beliefs', count: 12 })),
    ).toBeInTheDocument();
    expect(
      within(host).getByText(
        prettyJsonText([{ goal: 'clear the dawn berths', status: 'active' }]),
      ),
    ).toBeInTheDocument();
  });

  it('(a) shows every populated nine-field key and omits unpopulated ones (PD-16)', () => {
    render(<MentalSurfacingFixtures />);
    const host = screen.getByTestId('mental-character-populated-host');
    for (const label of ['Identity', 'Attention', 'Norms', 'Constraints']) {
      expect(within(host).getByText(label)).toBeInTheDocument();
    }
    expect(within(host).queryByText('Intentions')).not.toBeInTheDocument();
    expect(within(host).queryByText('Dispositions')).not.toBeInTheDocument();
  });

  it('(b) character without modules.mental renders no mental section — no empty panel', () => {
    render(<MentalSurfacingFixtures />);
    const host = screen.getByTestId('mental-character-absent-host');
    expect(within(host).getByText('Ana')).toBeInTheDocument();
    expect(within(host).queryByTestId('mental-state-section')).not.toBeInTheDocument();
    expect(within(host).queryByText('Mental State')).not.toBeInTheDocument();
    expect(within(host).queryByText('Beliefs')).not.toBeInTheDocument();
  });

  it('(c) event with recorded observation shows observers as name + id (PD-18)', () => {
    render(<MentalSurfacingFixtures />);
    const host = screen.getByTestId('mental-event-observers-host');
    expect(within(host).getByTestId('event-observers-line')).toBeInTheDocument();
    expect(within(host).getByText('Observers:')).toBeInTheDocument();
    expect(within(host).getByText('Ana (kb_ana)')).toBeInTheDocument();
  });

  it('(d) event with observers: [] renders the explicit "No observers" line (PD-9)', () => {
    render(<MentalSurfacingFixtures />);
    const host = screen.getByTestId('mental-event-empty-host');
    expect(within(host).getByTestId('event-observers-line')).toBeInTheDocument();
    expect(within(host).getByText('Observers:')).toBeInTheDocument();
    expect(within(host).getByText('No observers')).toBeInTheDocument();
  });

  it('(e) event without observation hides the observers section entirely (PD-9 absent = unrecorded)', () => {
    render(<MentalSurfacingFixtures />);
    const host = screen.getByTestId('mental-event-absent-host');
    expect(within(host).queryByTestId('event-observers-line')).not.toBeInTheDocument();
    expect(within(host).queryByText('Observers:')).not.toBeInTheDocument();
  });
});

describe('MentalSurfacingFixtures render — dark theme', () => {
  it('renders all five states under .dark without throw, populated fields still visible', () => {
    document.documentElement.classList.add('dark');
    expect(() => render(<MentalSurfacingFixtures />)).not.toThrow();
    expect(screen.getByTestId('mental-surfacing-fixtures')).toBeInTheDocument();
    const populated = screen.getByTestId('mental-character-populated-host');
    expect(within(populated).getByText('Mental State')).toBeInTheDocument();
    expect(within(populated).getByText('Beliefs')).toBeInTheDocument();
    // Absent mental still hides the section in dark.
    const absent = screen.getByTestId('mental-character-absent-host');
    expect(within(absent).queryByTestId('mental-state-section')).not.toBeInTheDocument();
    // Observers list + explicit empty claim still render in dark.
    const observed = screen.getByTestId('mental-event-observers-host');
    expect(within(observed).getByText('Ana (kb_ana)')).toBeInTheDocument();
    const empty = screen.getByTestId('mental-event-empty-host');
    expect(within(empty).getByText('No observers')).toBeInTheDocument();
    // Absent observation still hidden in dark.
    const unrecorded = screen.getByTestId('mental-event-absent-host');
    expect(within(unrecorded).queryByTestId('event-observers-line')).not.toBeInTheDocument();
  });
});
