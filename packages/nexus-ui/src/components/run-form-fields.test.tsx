import { fireEvent, render, screen, within } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, expect, it, vi } from 'vitest';

import {
  RunFormFields,
  type InvocationSchema,
  type RunFormCopy,
} from './run-form-fields';

const COPY: RunFormCopy = {
  emptyTitle: 'Can’t run this module',
  emptyDescription: 'Manifest is missing fields needed to build a form.',
  unsupportedFieldNote: 'Not available in the guided form — use the Advanced JSON editor.',
  entityPlaceholder: 'Choose an entry',
  selectPlaceholder: 'Choose a value',
  entityEmptyTitle: 'No characters to run',
  entityEmptyDescription: 'Add character knowledge entries in this World, then return.',
};

const BASIC_COMBAT_SCHEMA: InvocationSchema = {
  type: 'object',
  properties: {
    attacker_id: { type: 'string' },
    defender_id: { type: 'string' },
  },
};

const KITCHEN_SINK_SCHEMA: InvocationSchema = {
  type: 'object',
  properties: {
    mode: { type: 'string', enum: ['quick', 'full'], title: 'Mode' },
    rounds: { type: 'integer', minimum: 1, maximum: 20, title: 'Rounds' },
    allow_items: { type: 'boolean', title: 'Allow items' },
    note: { type: 'string', description: 'Freeform note for the run.' },
    tags: { type: 'array', items: { type: 'string' } },
  },
  required: ['mode'],
};

describe('RunFormFields', () => {
  it('derives two entity pickers from the basic-combat invocation schema', () => {
    const onChange = vi.fn();
    const { container } = render(
      <RunFormFields
        schema={BASIC_COMBAT_SCHEMA}
        requiredKeyBlockTypes={['character']}
        values={{}}
        onChange={onChange}
        entityEntries={{
          attacker_id: [{ id: 'char-aria', title: 'Aria' }],
          defender_id: [{ id: 'char-brann', title: 'Brann' }],
        }}
        copy={COPY}
      />,
    );

    expect(container.querySelector('[data-field="attacker_id"]')).not.toBeNull();
    expect(container.querySelector('[data-field="defender_id"]')).not.toBeNull();
    expect(screen.getByText('Attacker')).toBeInTheDocument();
    expect(screen.getByText('Defender')).toBeInTheDocument();

    fireEvent.change(screen.getAllByTestId('entity-picker-select')[0], {
      target: { value: 'char-aria' },
    });
    expect(onChange).toHaveBeenCalledWith('attacker_id', 'char-aria');
  });

  it('renders picker empty state when no entries are available', () => {
    render(
      <RunFormFields
        schema={BASIC_COMBAT_SCHEMA}
        requiredKeyBlockTypes={['character']}
        values={{}}
        onChange={() => {}}
        copy={COPY}
      />,
    );

    expect(screen.getAllByTestId('entity-picker-empty')).toHaveLength(2);
    expect(screen.getAllByText('No characters to run')).toHaveLength(2);
  });

  it('derives enum/number/boolean/string controls and flags unsupported kinds', () => {
    const onChange = vi.fn();
    render(
      <RunFormFields
        schema={KITCHEN_SINK_SCHEMA}
        values={{ rounds: 3, allow_items: true }}
        onChange={onChange}
        copy={COPY}
      />,
    );

    // enum → select with options
    const mode = screen.getByTestId('run-form-field-mode');
    expect(mode.tagName).toBe('SELECT');
    expect(within(mode).getByText('quick')).toBeInTheDocument();
    // required marker
    expect(screen.getByText('Mode').parentElement?.textContent).toContain('*');

    // number → input[type=number] with min/max
    const rounds = screen.getByTestId('run-form-field-rounds');
    expect(rounds).toHaveAttribute('type', 'number');
    expect(rounds).toHaveAttribute('min', '1');
    expect(rounds).toHaveAttribute('max', '20');

    // boolean → checkbox
    const allowItems = screen.getByTestId('run-form-field-allow_items');
    expect(allowItems).toHaveAttribute('type', 'checkbox');
    expect(allowItems).toBeChecked();

    // string → text input with description helper
    expect(screen.getByTestId('run-form-field-note')).toBeInTheDocument();
    expect(screen.getByText('Freeform note for the run.')).toBeInTheDocument();

    // array → unsupported note
    expect(screen.getByTestId('run-form-field-tags-unsupported')).toHaveTextContent(
      'Not available in the guided form',
    );

    fireEvent.change(mode, { target: { value: 'full' } });
    expect(onChange).toHaveBeenCalledWith('mode', 'full');
    fireEvent.click(allowItems);
    expect(onChange).toHaveBeenCalledWith('allow_items', false);
  });

  it('renders the caller-owned empty state when the schema is missing', () => {
    render(<RunFormFields schema={null} values={{}} onChange={() => {}} copy={COPY} />);

    expect(screen.getByTestId('run-form-empty')).toBeInTheDocument();
    expect(screen.getByText('Can’t run this module')).toBeInTheDocument();
  });

  it('falls back to text input for plain string fields without required types', () => {
    render(
      <RunFormFields
        schema={{ properties: { session_id: { type: 'string' } } }}
        values={{}}
        onChange={() => {}}
        copy={COPY}
      />,
    );

    // `_id` name but no required_key_block_types → plain text input, not a picker.
    expect(screen.getByTestId('run-form-field-session_id')).toBeInTheDocument();
    expect(screen.queryByTestId('entity-picker-select')).not.toBeInTheDocument();
  });
});
