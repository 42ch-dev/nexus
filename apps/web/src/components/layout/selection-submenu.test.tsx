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

  it('dismisses on Tab out via onClose', () => {
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
      menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }));
      expect(onClose).toHaveBeenCalled();
    } finally {
      document.body.removeChild(anchor);
    }
  });

  // V1.127 P0 T4 (AC-V1127-4) — dismiss on layout change. The popover is
  // position:fixed and its coordinates are derived once from the anchor rect,
  // so resize/scroll moves the anchor under the popover. Dismiss (not
  // reposition) is the chosen UX — submenu is cheap to reopen.
  describe('layout-change dismiss (V1.127 P0 T4)', () => {
    it('dismisses on window resize via onClose', () => {
      const anchor = document.createElement('button');
      document.body.appendChild(anchor);
      const onClose = vi.fn();
      try {
        render(
          <SelectionSubmenu
            items={[{ id: 'a', label: 'Action A', onSelect: () => {} }]}
            open
            onClose={onClose}
            anchorEl={anchor}
            ariaLabel="Test menu"
          />,
        );

        expect(screen.getByRole('menu', { name: 'Test menu' })).toBeInTheDocument();
        window.dispatchEvent(new Event('resize'));
        expect(onClose).toHaveBeenCalledTimes(1);
      } finally {
        document.body.removeChild(anchor);
      }
    });

    it('dismisses on scroll of an ancestor scroll container via onClose (capture listener)', () => {
      // Mirrors the sidebar chrome's `overflow-auto` <ul>: the submenu anchor
      // lives inside a scroll container that is NOT `window`. `scroll` does
      // not bubble, so the listener captures on `window` — the capture phase
      // descends root → target, so a scroll on any descendant reaches it.
      const scrollContainer = document.createElement('div');
      const anchor = document.createElement('button');
      scrollContainer.appendChild(anchor);
      document.body.appendChild(scrollContainer);
      const onClose = vi.fn();
      try {
        render(
          <SelectionSubmenu
            items={[{ id: 'a', label: 'Action A', onSelect: () => {} }]}
            open
            onClose={onClose}
            anchorEl={anchor}
            ariaLabel="Test menu"
          />,
        );

        expect(screen.getByRole('menu', { name: 'Test menu' })).toBeInTheDocument();
        scrollContainer.dispatchEvent(new Event('scroll'));
        expect(onClose).toHaveBeenCalledTimes(1);
      } finally {
        document.body.removeChild(scrollContainer);
      }
    });

    it('does not dismiss on unrelated events (mousemove)', () => {
      const anchor = document.createElement('button');
      document.body.appendChild(anchor);
      const onClose = vi.fn();
      try {
        render(
          <SelectionSubmenu
            items={[{ id: 'a', label: 'Action A', onSelect: () => {} }]}
            open
            onClose={onClose}
            anchorEl={anchor}
            ariaLabel="Test menu"
          />,
        );

        expect(screen.getByRole('menu', { name: 'Test menu' })).toBeInTheDocument();
        window.dispatchEvent(new MouseEvent('mousemove'));
        expect(onClose).not.toHaveBeenCalled();
      } finally {
        document.body.removeChild(anchor);
      }
    });
  });
});