/**
 * Validate Preset dialog — the dry-run body over the real service route shape.
 *
 * The core derives `valid` from errors alone and reports warnings separately,
 * so a valid verdict with warnings must render BOTH; the earlier body returned
 * early on `valid` and silently dropped them.
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { ValidatePresetDialog } from '@/pages/dialogs/validate-preset-dialog';
import { BrowserClient } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';

function renderDialog() {
  return renderInApp(
    <ValidatePresetDialog open onOpenChange={() => undefined} initialPath="/home/a/.nexus42/presets/p/preset.yaml" />,
    { client: new BrowserClient() },
  );
}

function validationHandler(body: Record<string, unknown>) {
  return http.post('/v1/daemon/presets:validate', () => HttpResponse.json(body));
}

describe('ValidatePresetDialog', () => {
  it('renders warnings beside a valid verdict', async () => {
    useHandlers(
      validationHandler({
        valid: true,
        errors: [],
        warnings: ['run_intents is empty', 'state start has no description'],
        state_count: 2,
      }),
    );

    renderDialog();
    const dialog = await screen.findByRole('dialog');
    // The field is reachable through its own label (id derived per instance).
    expect(within(dialog).getByLabelText('Preset path')).toHaveValue(
      '/home/a/.nexus42/presets/p/preset.yaml',
    );
    await userEvent.click(within(dialog).getByRole('button', { name: 'Validate' }));

    expect(await within(dialog).findByTestId('preset-valid')).toHaveTextContent(
      'Preset is valid',
    );
    expect(within(dialog).getByText('Warnings')).toBeInTheDocument();
    expect(within(dialog).getByText('run_intents is empty')).toBeInTheDocument();
    expect(within(dialog).getByText('state start has no description')).toBeInTheDocument();
  });

  it('renders a clean success without a warnings block', async () => {
    useHandlers(validationHandler({ valid: true, errors: [], warnings: [], state_count: 1 }));

    renderDialog();
    const dialog = await screen.findByRole('dialog');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Validate' }));

    expect(await within(dialog).findByTestId('preset-valid')).toBeInTheDocument();
    expect(within(dialog).queryByText('Warnings')).not.toBeInTheDocument();
  });

  it('renders every error of a failed validation', async () => {
    useHandlers(
      validationHandler({
        valid: false,
        errors: ['preset.initial: unknown state "start"'],
        warnings: [],
      }),
    );

    renderDialog();
    const dialog = await screen.findByRole('dialog');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Validate' }));

    expect(await within(dialog).findByTestId('preset-invalid')).toHaveTextContent(
      'Validation failed',
    );
    expect(within(dialog).getByText('preset.initial: unknown state "start"')).toBeInTheDocument();
    expect(within(dialog).queryByTestId('preset-valid')).not.toBeInTheDocument();
  });
});
