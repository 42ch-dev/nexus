import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { FormSection } from './form-section';

/**
 * V1.149 P1 — L2 pattern component (DESIGN.md components.form-section).
 *
 * Pins: the field-group layout (stacked, space-4 rhythm), native fieldset/
 * legend group semantics, caller-owned description, and the inline error
 * slot (role=alert + error-surface tokens). Field controls, validation, and
 * copy stay caller-owned.
 */
describe('FormSection', () => {
  it('groups fields under a legend with the heading voice', () => {
    render(
      <FormSection title="Connection">
        <label htmlFor="host">Host</label>
        <input id="host" name="host" />
      </FormSection>,
    );

    expect(screen.getByRole('group', { name: 'Connection' })).toBeInTheDocument();
    const legend = screen.getByText('Connection');
    expect(legend.tagName).toBe('LEGEND');
    expect(legend.className).toMatch(/\btext-heading-16\b/);
    expect(screen.getByLabelText('Host')).toBeInTheDocument();
  });

  it('stacks fields with the space-4 rhythm', () => {
    const { container } = render(
      <FormSection title="Connection">
        <input name="host" />
        <input name="port" />
      </FormSection>,
    );

    const stack = container.querySelector('.flex.flex-col.gap-4');
    expect(stack).not.toBeNull();
    expect(stack!.children.length).toBe(2);
  });

  it('renders the caller-owned description', () => {
    render(
      <FormSection title="Connection" description="Local daemon endpoint.">
        <input name="host" />
      </FormSection>,
    );

    expect(screen.getByText('Local daemon endpoint.')).toBeInTheDocument();
  });

  it('renders the inline error slot with the error-surface recipe', () => {
    render(
      <FormSection title="Connection" error="Host is required.">
        <input name="host" aria-label="Host" />
      </FormSection>,
    );

    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('Host is required.');
    expect(alert.className).toMatch(/\bbg-error-surface\b/);
    expect(alert.className).toMatch(/\bborder-error-surface-border\b/);
  });

  it('omits the error slot when the caller passes no error', () => {
    render(
      <FormSection title="Connection">
        <input name="host" aria-label="Host" />
      </FormSection>,
    );

    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('merges an optional className onto the fieldset', () => {
    const { container } = render(
      <FormSection title="Connection" className="rounded-card border border-gray-alpha-400 p-4">
        <input name="host" aria-label="Host" />
      </FormSection>,
    );

    expect((container.firstChild as HTMLElement).className).toMatch(/\brounded-card\b/);
  });
});
