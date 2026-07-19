import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { SelectionSubmenu } from '@/components/selection-submenu/selection-submenu';

describe('SelectionSubmenu', () => {
  it('renders nothing when closed', () => {
    const { container } = render(
      <SelectionSubmenu
        items={[]}
        open={false}
        onClose={() => {}}
        anchorEl={null}
        ariaLabel="Test menu"
      />,
    );
    expect(container.innerHTML).toBe('');
  });

  it('renders items when open with anchor', () => {
    const anchor = document.createElement('button');
    document.body.appendChild(anchor);
    try {
      render(
        <SelectionSubmenu
          items={[
            { id: 'a', label: 'Action A', onSelect: () => {} },
            { id: 'b', label: 'Action B', onSelect: () => {} },
          ]}
          open
          onClose={() => {}}
          anchorEl={anchor}
          ariaLabel="Test menu"
        />,
      );

      expect(screen.getByRole('menu', { name: 'Test menu' })).toBeInTheDocument();
      expect(screen.getByRole('menuitem', { name: 'Action A' })).toBeInTheDocument();
      expect(screen.getByRole('menuitem', { name: 'Action B' })).toBeInTheDocument();
    } finally {
      document.body.removeChild(anchor);
    }
  });

  it('dismisses on Escape via onClose', async () => {
    const anchor = document.createElement('button');
    document.body.appendChild(anchor);
    const onClose = vi.fn();
    try {
      render(
        <SelectionSubmenu
          items={[
            { id: 'a', label: 'Action A', onSelect: () => {} },
          ]}
          open
          onClose={onClose}
          anchorEl={anchor}
          ariaLabel="Test menu"
        />,
      );

      const menu = screen.getByRole('menu', { name: 'Test menu' });
      menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      expect(onClose).toHaveBeenCalled();
    } finally {
      document.body.removeChild(anchor);
    }
  });
});