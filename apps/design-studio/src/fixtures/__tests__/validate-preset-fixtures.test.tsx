import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ValidatePresetFixtures } from '@/fixtures/validate-preset-fixtures';

/**
 * The Validate Preset body has four states; the warning state is the one the
 * product dialog previously got wrong (it returned early on `valid` and hid
 * the core's warnings), so it is asserted as a first-class state here.
 */
describe('ValidatePresetFixtures', () => {
  it('mounts pending, success, warning and error states', () => {
    render(<ValidatePresetFixtures />);

    for (const state of ['pending', 'success', 'warning', 'error']) {
      expect(screen.getByTestId(`validate-preset-fixture-${state}`)).toBeInTheDocument();
    }
    expect(screen.getByTestId('validate-preset-fixtures')).toBeInTheDocument();
  });

  it('renders the pending submit state', () => {
    render(<ValidatePresetFixtures />);
    const pending = screen.getByTestId('validate-preset-fixture-pending');

    const submit = within(pending).getByRole('button', { name: 'Validating preset…' });
    expect(submit).toBeDisabled();
    expect(within(pending).getByRole('textbox')).toHaveValue(
      '/home/author/.nexus42/presets/novel-writing/preset.yaml',
    );
  });

  it('gives every mounted body its own label/input id pair', () => {
    render(<ValidatePresetFixtures />);

    const ids = ['pending', 'success', 'warning', 'error'].map((state) => {
      const frame = screen.getByTestId(`validate-preset-fixture-${state}`);
      // getByLabelText resolves through htmlFor → id: each label reaches the
      // input rendered beside it, not the first body in the document.
      return within(frame).getByLabelText('Preset path').id;
    });

    expect(new Set(ids).size).toBe(4);
    for (const id of ids) {
      expect(id.length).toBeGreaterThan(0);
      expect(document.querySelectorAll(`[id="${id}"]`)).toHaveLength(1);
    }
  });

  it('renders success and error verdicts', () => {
    render(<ValidatePresetFixtures />);

    expect(within(screen.getByTestId('validate-preset-fixture-success')).getByTestId('preset-valid'))
      .toHaveTextContent('Preset is valid');
    expect(within(screen.getByTestId('validate-preset-fixture-error')).getByTestId('preset-invalid'))
      .toHaveTextContent('Validation failed');
  });

  it('renders warnings beside a valid verdict, not instead of it', () => {
    render(<ValidatePresetFixtures />);
    const warning = screen.getByTestId('validate-preset-fixture-warning');

    expect(within(warning).getByTestId('preset-valid')).toHaveTextContent('Preset is valid');
    expect(within(warning).getByText('Warnings')).toBeInTheDocument();
    expect(
      within(warning).getByText(/run_intents is empty/),
    ).toBeInTheDocument();
  });
});
