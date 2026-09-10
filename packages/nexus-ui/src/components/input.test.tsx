import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Input } from './input';

describe('Input', () => {
  // --- base rendering ---

  it('renders an input element', () => {
    render(<Input data-testid="test-input" />);
    const input = screen.getByTestId('test-input');
    expect(input).toHaveProperty('tagName', 'INPUT');
  });


  // --- className merge (cn integration) ---

  it('merges custom className with base classes', () => {
    render(<Input className="custom-class" data-testid="test-input" />);
    const input = screen.getByTestId('test-input');
    expect(input).toHaveClass('custom-class');
  });

  // --- invalid prop → aria-invalid + visual state ---

  it('sets aria-invalid="true" when invalid is true', () => {
    render(<Input invalid data-testid="test-input" />);
    const input = screen.getByTestId('test-input');
    expect(input).toHaveAttribute('aria-invalid', 'true');
  });

  it('does not set aria-invalid when invalid is false', () => {
    render(<Input invalid={false} data-testid="test-input" />);
    const input = screen.getByTestId('test-input');
    expect(input).not.toHaveAttribute('aria-invalid');
  });

  it('does not set aria-invalid when invalid is omitted', () => {
    render(<Input data-testid="test-input" />);
    const input = screen.getByTestId('test-input');
    expect(input).not.toHaveAttribute('aria-invalid');
  });



  // --- ref-as-prop ---

  it('passes the ref to the underlying input element', () => {
    let ref: HTMLInputElement | null = null;
    const setRef = (el: HTMLInputElement | null) => {
      ref = el;
    };
    render(<Input ref={setRef} data-testid="test-input" />);
    expect(ref).not.toBeNull();
    expect(ref!).toHaveProperty('tagName', 'INPUT');
    expect(ref).toBe(screen.getByTestId('test-input'));
  });

  // --- accessibility-relevant attributes ---

  it('passes through standard aria-describedby', () => {
    render(<Input aria-describedby="helper-1 error-1" data-testid="test-input" />);
    const input = screen.getByTestId('test-input');
    expect(input).toHaveAttribute('aria-describedby', 'helper-1 error-1');
  });

  it('passes through standard id', () => {
    render(<Input id="my-field" data-testid="test-input" />);
    const input = screen.getByTestId('test-input');
    expect(input).toHaveAttribute('id', 'my-field');
  });

  it('passes through native required attribute', () => {
    render(<Input required data-testid="test-input" />);
    const input = screen.getByTestId('test-input');
    expect(input).toBeRequired();
  });

  it('passes through aria-required', () => {
    render(<Input aria-required="true" data-testid="test-input" />);
    const input = screen.getByTestId('test-input');
    expect(input).toHaveAttribute('aria-required', 'true');
  });

  it('does not render helper/error text (app-owned)', () => {
    const { container } = render(<Input data-testid="test-input" />);
    expect(container.querySelectorAll('p').length).toBe(0);
    expect(container.querySelectorAll('[role="alert"]').length).toBe(0);
  });

  it('exposes the native disabled state', () => {
    render(<Input disabled data-testid="test-input" />);
    expect(screen.getByTestId('test-input')).toBeDisabled();
  });
});
