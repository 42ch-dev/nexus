/**
 * Validate Preset dialog i18n + validation result tests.
 *
 * Exercises the dry-run form against msw: enter a path, submit, and assert the
 * daemon receives a POST `/v1/daemon/presets:validate` and the dialog surfaces
 * the translated result states.
 */
import { http, HttpResponse } from 'msw';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { BrowserClient } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';
import { ValidatePresetDialog } from '@/pages/dialogs/validate-preset-dialog';

function renderDialog(initialPath?: string) {
  const onOpenChange = vi.fn();
  renderInApp(
    <ValidatePresetDialog open onOpenChange={onOpenChange} initialPath={initialPath} />,
    { client: new BrowserClient() },
  );
  return { onOpenChange };
}

describe('ValidatePresetDialog', () => {
  it('renders translated labels', () => {
    renderDialog();

    expect(screen.getByRole('heading', { name: 'Validate Preset' })).toBeInTheDocument();
    expect(screen.getByLabelText(/^Preset path$/i)).toBeInTheDocument();
    expect(
      screen.getByText('Resolved by the daemon against the local home layout.'),
    ).toBeInTheDocument();
  });

  it('submits a well-formed POST /v1/daemon/presets:validate', async () => {
    const user = userEvent.setup();
    let postedBody: unknown = null;
    useHandlers(
      http.post('/v1/daemon/presets:validate', async ({ request }) => {
        postedBody = await request.json();
        return HttpResponse.json({
          valid: true,
          errors: [],
          state_count: 4,
        });
      }),
    );

    renderDialog('/presets/foo.yaml');

    const input = screen.getByLabelText(/^Preset path$/i);
    expect(input).toHaveValue('/presets/foo.yaml');
    await user.click(screen.getByRole('button', { name: /^Validate$/i }));

    await waitFor(() => expect(postedBody).not.toBeNull());
    expect(postedBody).toEqual({ path: '/presets/foo.yaml' });
    expect(await screen.findByText('Preset is valid')).toBeInTheDocument();
    expect(screen.getByText('Safe to commit · 4 states.')).toBeInTheDocument();
  });

  it('disables submit when the path is empty', async () => {
    renderDialog();

    const input = screen.getByLabelText(/^Preset path$/i);
    await userEvent.clear(input);

    expect(screen.getByRole('button', { name: /^Validate$/i })).toBeDisabled();
  });

  it('surfaces errors and warnings inline', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.post('/v1/daemon/presets:validate', () =>
        HttpResponse.json({
          valid: false,
          errors: ['Missing initial state'],
          warnings: ['No exit transitions'],
        }),
      ),
    );

    renderDialog();

    await user.type(screen.getByLabelText(/^Preset path$/i), '/presets/bad.yaml');
    await user.click(screen.getByRole('button', { name: /^Validate$/i }));

    expect(await screen.findByText('Validation failed')).toBeInTheDocument();
    expect(screen.getByText('Missing initial state')).toBeInTheDocument();
    expect(screen.getByText('Warnings')).toBeInTheDocument();
    expect(screen.getByText('No exit transitions')).toBeInTheDocument();
  });

  it('closes when Close is clicked', async () => {
    const user = userEvent.setup();
    const { onOpenChange } = renderDialog();

    await user.click(screen.getByRole('button', { name: /^Close$/i }));

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
