import { fireEvent, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, expect, it, vi } from 'vitest';

import { EntityPickerField, type EntityPickerEntry } from './entity-picker-field';

const ENTRIES: EntityPickerEntry[] = [
  { id: 'char-aria', title: 'Aria', subtitle: 'Level 3' },
  { id: 'char-brann', title: 'Brann' },
];

describe('EntityPickerField', () => {
  it('associates the label with the select via htmlFor/id', () => {
    render(
      <EntityPickerField
        id="attacker"
        label="Attacker"
        entries={ENTRIES}
        value={null}
        onChange={() => {}}
        placeholder="Choose a character"
      />,
    );

    const select = screen.getByTestId('entity-picker-select');
    expect(select).toHaveAttribute('id', 'attacker');
    expect(screen.getByText('Attacker')).toHaveAttribute('for', 'attacker');
  });

  it('lists entries and reports selection through onChange', () => {
    const onChange = vi.fn();
    render(
      <EntityPickerField
        id="defender"
        label="Defender"
        entries={ENTRIES}
        value={null}
        onChange={onChange}
        placeholder="Choose a character"
      />,
    );

    const select = screen.getByTestId('entity-picker-select');
    expect(select).toHaveTextContent('Aria — Level 3');
    expect(select).toHaveTextContent('Brann');

    fireEvent.change(select, { target: { value: 'char-brann' } });
    expect(onChange).toHaveBeenCalledWith('char-brann');
  });

  it('renders the caller-owned empty state instead of a select when entries are empty', () => {
    render(
      <EntityPickerField
        id="attacker"
        label="Attacker"
        entries={[]}
        value={null}
        onChange={() => {}}
        emptyTitle="No characters to run"
        emptyDescription="Add character knowledge entries in this World, then return."
      />,
    );

    expect(screen.getByTestId('entity-picker-empty')).toBeInTheDocument();
    expect(screen.getByText('No characters to run')).toBeInTheDocument();
    expect(screen.queryByTestId('entity-picker-select')).not.toBeInTheDocument();
  });

  it('marks required fields and maps invalid to aria-invalid', () => {
    render(
      <EntityPickerField
        id="attacker"
        label="Attacker"
        entries={ENTRIES}
        value="char-aria"
        onChange={() => {}}
        required
        invalid
      />,
    );

    expect(screen.getByText('*')).toBeInTheDocument();
    expect(screen.getByTestId('entity-picker-select')).toHaveAttribute('aria-invalid', 'true');
  });
});
