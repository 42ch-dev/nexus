import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Label } from './label';

describe('Label', () => {
  // --- base rendering ---

  it('renders a label element', () => {
    render(<Label data-testid="test-label">Field Name</Label>);
    const label = screen.getByTestId('test-label');
    expect(label).toHaveProperty('tagName', 'LABEL');
  });

  it('renders with base structural classes', () => {
    render(<Label data-testid="test-label">Field</Label>);
    const label = screen.getByTestId('test-label');
    expect(label).toHaveClass('text-label-14');
    expect(label).toHaveClass('font-medium');
    expect(label).toHaveClass('text-gray-1000');
  });

  it('renders its children', () => {
    render(<Label>Email Address</Label>);
    expect(screen.getByText('Email Address')).toBeInTheDocument();
  });

  // --- className merge (cn integration) ---

  it('merges custom className with base classes', () => {
    render(<Label className="custom-class" data-testid="test-label">Field</Label>);
    const label = screen.getByTestId('test-label');
    expect(label).toHaveClass('custom-class');
    expect(label).toHaveClass('text-label-14'); // base class still present
  });

  // --- htmlFor passthrough ---

  it('passes through htmlFor to the label element', () => {
    render(<Label htmlFor="my-input" data-testid="test-label">Field</Label>);
    const label = screen.getByTestId('test-label');
    expect(label).toHaveAttribute('for', 'my-input');
  });

  // --- ref-as-prop ---

  it('passes the ref to the underlying label element', () => {
    let ref: HTMLLabelElement | null = null;
    const setRef = (el: HTMLLabelElement | null) => {
      ref = el;
    };
    render(<Label ref={setRef} data-testid="test-label">Field</Label>);
    expect(ref).not.toBeNull();
    expect(ref!).toHaveProperty('tagName', 'LABEL');
    expect(ref).toBe(screen.getByTestId('test-label'));
  });

  // --- no ID generation (app-owned) ---

  it('does not auto-generate an id', () => {
    render(<Label data-testid="test-label">Field</Label>);
    const label = screen.getByTestId('test-label');
    expect(label).not.toHaveAttribute('id');
  });

  // --- no required/optional copy emitted (app-owned) ---

  it('does not render required/optional indicators as children', () => {
    const { container } = render(<Label data-testid="test-label">Name</Label>);
    expect(container.querySelectorAll('[aria-label*="required"]').length).toBe(0);
    expect(container.textContent).toBe('Name');
  });
});
