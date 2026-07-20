import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import {
  CreatorEntitySelectionProvider,
  useCreatorEntitySelection,
} from './creator-entity-selection-context';

function Probe() {
  const { selectedEntity, setSelectedEntity, clearSelectedEntity } = useCreatorEntitySelection();
  return (
    <div>
      <span data-testid="selected">{selectedEntity?.id ?? 'none'}</span>
      <button type="button" onClick={() => setSelectedEntity({ kind: 'work', id: 'w1', label: 'Novel' })}>
        Select
      </button>
      <button type="button" onClick={clearSelectedEntity}>
        Clear
      </button>
    </div>
  );
}

describe('CreatorEntitySelectionProvider', () => {
  it('stores and clears selectedEntity', () => {
    render(
      <CreatorEntitySelectionProvider>
        <Probe />
      </CreatorEntitySelectionProvider>,
    );

    expect(screen.getByTestId('selected')).toHaveTextContent('none');
    fireEvent.click(screen.getByRole('button', { name: 'Select' }));
    expect(screen.getByTestId('selected')).toHaveTextContent('w1');
    fireEvent.click(screen.getByRole('button', { name: 'Clear' }));
    expect(screen.getByTestId('selected')).toHaveTextContent('none');
  });
});
