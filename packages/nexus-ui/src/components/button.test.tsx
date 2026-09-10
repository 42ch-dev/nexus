import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Button } from './button';

describe('Button', () => {
  // --- variant rendering ---





  // --- size rendering ---





  // --- asChild prop (Radix Slot delegation) ---

  it('renders as a <button> element by default', () => {
    render(<Button>Click</Button>);
    const el = screen.getByRole('button', { name: 'Click' });
    expect(el.tagName).toBe('BUTTON');
  });

  it('delegates to child element when asChild is true', () => {
    render(
      <Button asChild>
        <a href="/settings">Settings</a>
      </Button>,
    );
    const link = screen.getByRole('link', { name: 'Settings' });
    expect(link).toBeInTheDocument();
    expect(link.tagName).toBe('A');
    expect(link).toHaveAttribute('href', '/settings');
  });

  // --- disabled state ---

  it('exposes the native disabled state', () => {
    render(<Button disabled>Blocked</Button>);
    expect(screen.getByRole('button', { name: 'Blocked' })).toBeDisabled();
  });

  // --- className merge (cn integration) ---

  it('merges custom className with variant classes', () => {
    render(
      <Button variant="primary" className="custom-extra">
        Styled
      </Button>,
    );
    const btn = screen.getByRole('button', { name: 'Styled' });
    expect(btn).toHaveClass('custom-extra');
  });

  // --- ref-as-prop ---

  it('passes the ref to the underlying button element', () => {
    let ref: HTMLButtonElement | null = null;
    const setRef = (el: HTMLButtonElement | null) => {
      ref = el;
    };
    render(<Button ref={setRef}>Ref</Button>);
    expect(ref).not.toBeNull();
    expect(ref!).toHaveProperty('tagName', 'BUTTON');
  });
});
