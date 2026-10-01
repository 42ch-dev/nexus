/**
 * Module editing Studio fixtures — boundary + six-state render tests
 * (v1.203 P2 O1/O2/O3, PR #355 Greptile P3 studio-first fix).
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { ModuleEditingFixtures } from '@/fixtures/module-editing-fixtures';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const FIXTURE_SOURCE_PATH = path.resolve(__dirname, '../module-editing-fixtures.tsx');

beforeEach(() => {
  document.documentElement.classList.remove('dark');
});

afterEach(() => {
  document.documentElement.classList.remove('dark');
});

describe('module-editing-fixtures presentational boundary', () => {
  it('does not import @xyflow/react, contracts, daemon clients, or useTranslation', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    const imports = source.match(/^import .*$/gm) ?? [];
    expect(imports.join('\n')).not.toMatch(/xyflow/);
    expect(imports.join('\n')).not.toMatch(/nexus-contracts/);
    expect(imports.join('\n')).not.toMatch(/useTranslation/);
    expect(imports.join('\n')).not.toMatch(/lib\/nexus/);
    expect(imports.join('\n')).not.toMatch(/NexusClient/);
  });

  it('mirrors the modules wire shape locally (additive optional modules bag)', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    expect(source).toMatch(/modules\?: Record<string, unknown>/);
  });
});

describe('ModuleEditingFixtures render — seven states (light)', () => {
  it('renders all seven fixture frames', () => {
    render(<ModuleEditingFixtures />);
    expect(screen.getByTestId('module-editing-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('module-fixture-mental-structured')).toBeInTheDocument();
    expect(screen.getByTestId('module-fixture-mental-raw')).toBeInTheDocument();
    expect(screen.getByTestId('module-fixture-belief-structured')).toBeInTheDocument();
    expect(screen.getByTestId('module-fixture-belief-raw')).toBeInTheDocument();
    expect(screen.getByTestId('module-fixture-observation-structured')).toBeInTheDocument();
    expect(screen.getByTestId('module-fixture-observation-empty')).toBeInTheDocument();
    expect(screen.getByTestId('module-fixture-observation-raw')).toBeInTheDocument();
  });

  it('(1) structured mental editing seeds the nine locked fields plus the unknown-key fallback row', () => {
    render(<ModuleEditingFixtures />);
    const host = screen.getByTestId('module-mental-structured-host');
    const section = within(host).getByTestId('mental-editing-section');
    expect(within(section).getByLabelText('Goals')).toHaveDisplayValue(/clear the dawn berths/);
    expect(within(section).getByLabelText('Emotions')).toHaveDisplayValue(/"alert"/);
    // Unknown own key outside the locked vocabulary — raw-JSON fallback row.
    expect(within(section).getByLabelText('custom_model_state')).toHaveDisplayValue(/deeply/);
    // Raw fallback textarea is NOT present in structured mode.
    expect(within(section).queryByTestId('fixture-mental-raw-json')).not.toBeInTheDocument();
  });

  it('(2) raw mental fallback exposes the complete stored array value', () => {
    render(<ModuleEditingFixtures />);
    const host = screen.getByTestId('module-mental-raw-host');
    const raw = within(host).getByTestId('fixture-mental-raw-json');
    expect(raw).toHaveDisplayValue(
      JSON.stringify([{ goal: 'legacy row the form cannot represent' }], null, 2),
    );
    // Structured fields stay inactive for a nonrepresentable seed.
    expect(within(host).queryByLabelText('Goals')).not.toBeInTheDocument();
  });

  it('(3) structured belief editing seeds every editable member of the stored row', () => {
    render(<ModuleEditingFixtures />);
    const host = screen.getByTestId('module-belief-structured-host');
    const section = within(host).getByTestId('belief-editing-section');
    expect(within(section).getByLabelText('Holder')).toHaveDisplayValue('kb_bo');
    expect(within(section).getByLabelText('Proposition')).toHaveDisplayValue(
      'the marble is in the box',
    );
    expect(within(section).getByLabelText('Truth Status')).toHaveDisplayValue('False');
    expect(within(section).getByLabelText('Order')).toHaveDisplayValue('1');
    expect(within(section).getByTestId('fixture-belief-add')).toBeInTheDocument();
  });

  it('(4) raw belief fallback exposes the complete stored non-array value', () => {
    render(<ModuleEditingFixtures />);
    const host = screen.getByTestId('module-belief-raw-host');
    const raw = within(host).getByTestId('fixture-belief-raw-json');
    expect(raw).toHaveDisplayValue(JSON.stringify('legacy scalar belief bag', null, 2));
    expect(within(host).queryByLabelText('Proposition')).not.toBeInTheDocument();
  });

  it('(5) structured observation editing seeds the observer selection + access JSON', () => {
    render(<ModuleEditingFixtures />);
    const host = screen.getByTestId('module-observation-structured-host');
    const section = within(host).getByTestId('observation-editing-section');
    expect(within(section).getByLabelText('Access')).toHaveDisplayValue(/line_of_sight/);
    const ana = within(section).getByLabelText('Ana (kb_ana)');
    expect(ana).toBeChecked();
    const bo = within(section).getByLabelText('Bo (kb_bo)');
    expect(bo).not.toBeChecked();
    expect(within(section).getByTestId('fixture-observation-add')).toBeInTheDocument();
  });

  it('(6) explicit empty observation renders the checked "no observers" claim (PD-9)', () => {
    render(<ModuleEditingFixtures />);
    const host = screen.getByTestId('module-observation-empty-host');
    const section = within(host).getByTestId('observation-editing-section');
    expect(within(section).getByTestId('fixture-observation-claim-none')).toBeChecked();
    // No observer is checked for the explicit-empty event.
    expect(within(section).getByLabelText('Ana (kb_ana)')).not.toBeChecked();
    expect(within(section).getByLabelText('Bo (kb_bo)')).not.toBeChecked();
  });

  it('(7) raw observation fallback exposes the complete stored non-object value', () => {
    render(<ModuleEditingFixtures />);
    const host = screen.getByTestId('module-observation-raw-host');
    const raw = within(host).getByTestId('fixture-observation-raw-json');
    expect(raw).toHaveDisplayValue('42');
    expect(within(host).queryByLabelText('Access')).not.toBeInTheDocument();
  });
});

describe('ModuleEditingFixtures render — dark theme', () => {
  it('renders all six states under .dark without throw, seeded values still visible', () => {
    document.documentElement.classList.add('dark');
    expect(() => render(<ModuleEditingFixtures />)).not.toThrow();
    expect(screen.getByTestId('module-editing-fixtures')).toBeInTheDocument();
    const mental = screen.getByTestId('module-mental-structured-host');
    expect(within(mental).getByLabelText('Goals')).toHaveDisplayValue(/clear the dawn berths/);
    const mentalRaw = screen.getByTestId('module-mental-raw-host');
    expect(within(mentalRaw).getByTestId('fixture-mental-raw-json')).toBeInTheDocument();
    const belief = screen.getByTestId('module-belief-structured-host');
    expect(within(belief).getByLabelText('Proposition')).toHaveDisplayValue(
      'the marble is in the box',
    );
    const observation = screen.getByTestId('module-observation-structured-host');
    expect(within(observation).getByLabelText('Ana (kb_ana)')).toBeChecked();
  });
});
