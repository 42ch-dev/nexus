import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Button } from './button';

describe('Button', () => {
  // --- variant rendering ---

  it('renders the primary variant with correct background and text classes', () => {
    render(<Button variant="primary">Save</Button>);
    const btn = screen.getByRole('button', { name: 'Save' });
    expect(btn).toHaveClass('bg-blue-700');
    expect(btn).toHaveClass('text-white');
    expect(btn).toHaveClass('dark:bg-brand-cyan');
    expect(btn).toHaveClass('dark:text-brand-deep-blue');
  });

  it('renders the secondary variant (default)', () => {
    render(<Button>Cancel</Button>);
    const btn = screen.getByRole('button', { name: 'Cancel' });
    expect(btn).toHaveClass('border');
    expect(btn).toHaveClass('bg-background-100');
    expect(btn).toHaveClass('text-gray-1000');
  });

  it('renders the tertiary variant with transparent background', () => {
    render(<Button variant="tertiary">Help</Button>);
    const btn = screen.getByRole('button', { name: 'Help' });
    expect(btn).toHaveClass('bg-transparent');
    expect(btn).toHaveClass('hover:bg-gray-alpha-100');
  });

  it('renders the destructive variant with red background', () => {
    render(<Button variant="destructive">Delete</Button>);
    const btn = screen.getByRole('button', { name: 'Delete' });
    expect(btn).toHaveClass('bg-red-800');
    expect(btn).toHaveClass('text-white');
    expect(btn).toHaveClass('dark:text-brand-deep-blue');
  });

  // --- size rendering ---

  it('renders with small size (32px height)', () => {
    render(<Button size="small">Go</Button>);
    const btn = screen.getByRole('button', { name: 'Go' });
    expect(btn).toHaveClass('h-8');
    expect(btn).toHaveClass('text-button-12');
  });

  it('renders with default size (40px height)', () => {
    render(<Button size="default">Submit</Button>);
    const btn = screen.getByRole('button', { name: 'Submit' });
    expect(btn).toHaveClass('h-10');
    expect(btn).toHaveClass('text-button-14');
  });

  it('renders with large size (48px height)', () => {
    render(<Button size="large">Confirm</Button>);
    const btn = screen.getByRole('button', { name: 'Confirm' });
    expect(btn).toHaveClass('h-12');
    expect(btn).toHaveClass('text-button-14');
  });

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
    // Slot should merge Button classes onto the child <a>
    expect(link).toHaveClass('inline-flex');
  });

  // --- disabled state ---

  it('applies disabled classes and attribute', () => {
    render(<Button disabled>Blocked</Button>);
    const btn = screen.getByRole('button', { name: 'Blocked' });
    expect(btn).toBeDisabled();
    expect(btn).toHaveClass('disabled:pointer-events-none');
    expect(btn).toHaveClass('disabled:bg-gray-100');
    expect(btn).toHaveClass('disabled:text-gray-700');
    expect(btn).toHaveClass('dark:disabled:bg-gray-100');
    expect(btn).toHaveClass('dark:disabled:text-gray-700');
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
    expect(btn).toHaveClass('bg-blue-700');
  });

  // --- base structural classes ---

  it('renders with base structural classes', () => {
    render(<Button>Base</Button>);
    const btn = screen.getByRole('button', { name: 'Base' });
    expect(btn).toHaveClass('inline-flex');
    expect(btn).toHaveClass('items-center');
    expect(btn).toHaveClass('rounded-control');
    expect(btn).toHaveClass('whitespace-nowrap');
  });

  // --- v0.4 motion tokens (hover/pressed states ease over duration-state) ---

  it('eases hover/pressed state changes over duration-state ease-standard', () => {
    render(<Button>Motion</Button>);
    const btn = screen.getByRole('button', { name: 'Motion' });
    expect(btn).toHaveClass('transition-colors');
    expect(btn).toHaveClass('duration-state');
    expect(btn).toHaveClass('ease-standard');
  });

  // --- forwardRef ---

  it('forwards the ref to the underlying button element', () => {
    let ref: HTMLButtonElement | null = null;
    const setRef = (el: HTMLButtonElement | null) => {
      ref = el;
    };
    render(<Button ref={setRef}>Ref</Button>);
    expect(ref).not.toBeNull();
    expect(ref!).toHaveProperty('tagName', 'BUTTON');
  });
});
