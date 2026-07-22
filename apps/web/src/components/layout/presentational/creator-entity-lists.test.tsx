import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { CreatorEntityLists } from './creator-entity-lists';

describe('CreatorEntityLists', () => {
  it('renders worlds and works sections with row selection', () => {
    const onSelectWork = vi.fn();

    render(
      <CreatorEntityLists
        labels={{ worldsTitle: 'Worlds', worksTitle: 'Works' }}
        worlds={[{ id: 'w1', label: 'Fantasy' }]}
        works={[{ id: 'work-1', label: 'Novel' }]}
        onSelectWork={onSelectWork}
        data-testid="entity-lists"
      />,
    );

    expect(screen.getByTestId('entity-lists-worlds-row-w1')).toHaveTextContent('Fantasy');
    fireEvent.click(screen.getByTestId('entity-lists-works-row-work-1'));
    expect(onSelectWork).toHaveBeenCalledWith('work-1');
  });
});
