import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Textarea } from './textarea';

describe('Textarea', () => {
  // --- base rendering ---

  it('renders a textarea element', () => {
    render(<Textarea data-testid="test-textarea" />);
    const textarea = screen.getByTestId('test-textarea');
    expect(textarea).toHaveProperty('tagName', 'TEXTAREA');
  });

  it('renders with base structural classes', () => {
    render(<Textarea data-testid="test-textarea" />);
    const textarea = screen.getByTestId('test-textarea');
    expect(textarea).toHaveClass('min-h-24');
    expect(textarea).toHaveClass('w-full');
    expect(textarea).toHaveClass('rounded-control');
    expect(textarea).toHaveClass('border');
    expect(textarea).toHaveClass('bg-background-100');
  });

  // --- className merge (cn integration) ---

  it('merges custom className with base classes', () => {
    render(<Textarea className="custom-class" data-testid="test-textarea" />);
    const textarea = screen.getByTestId('test-textarea');
    expect(textarea).toHaveClass('custom-class');
    expect(textarea).toHaveClass('min-h-24'); // base class still present
  });

  // --- invalid prop → aria-invalid + visual state ---

  it('sets aria-invalid="true" when invalid is true', () => {
    render(<Textarea invalid data-testid="test-textarea" />);
    const textarea = screen.getByTestId('test-textarea');
    expect(textarea).toHaveAttribute('aria-invalid', 'true');
  });

  it('does not set aria-invalid when invalid is false', () => {
    render(<Textarea invalid={false} data-testid="test-textarea" />);
    const textarea = screen.getByTestId('test-textarea');
    expect(textarea).not.toHaveAttribute('aria-invalid');
  });

  it('does not set aria-invalid when invalid is omitted', () => {
    render(<Textarea data-testid="test-textarea" />);
    const textarea = screen.getByTestId('test-textarea');
    expect(textarea).not.toHaveAttribute('aria-invalid');
  });

  it('applies red-700 border class when invalid', () => {
    render(<Textarea invalid data-testid="test-textarea" />);
    const textarea = screen.getByTestId('test-textarea');
    expect(textarea).toHaveClass('border-red-700');
    expect(textarea).not.toHaveClass('border-gray-alpha-400');
  });

  it('applies gray-alpha-400 border class when not invalid', () => {
    render(<Textarea data-testid="test-textarea" />);
    const textarea = screen.getByTestId('test-textarea');
    expect(textarea).toHaveClass('border-gray-alpha-400');
    expect(textarea).not.toHaveClass('border-red-700');
  });

  // --- forwardRef ---

  it('forwards the ref to the underlying textarea element', () => {
    let ref: HTMLTextAreaElement | null = null;
    const setRef = (el: HTMLTextAreaElement | null) => {
      ref = el;
    };
    render(<Textarea ref={setRef} data-testid="test-textarea" />);
    expect(ref).not.toBeNull();
    expect(ref!).toHaveProperty('tagName', 'TEXTAREA');
    expect(ref).toBe(screen.getByTestId('test-textarea'));
  });

  // --- accessibility-relevant attributes ---

  it('passes through standard aria-describedby', () => {
    render(<Textarea aria-describedby="helper-1 error-1" data-testid="test-textarea" />);
    const textarea = screen.getByTestId('test-textarea');
    expect(textarea).toHaveAttribute('aria-describedby', 'helper-1 error-1');
  });

  it('passes through standard id', () => {
    render(<Textarea id="my-field" data-testid="test-textarea" />);
    const textarea = screen.getByTestId('test-textarea');
    expect(textarea).toHaveAttribute('id', 'my-field');
  });

  it('passes through native required attribute', () => {
    render(<Textarea required data-testid="test-textarea" />);
    const textarea = screen.getByTestId('test-textarea');
    expect(textarea).toBeRequired();
  });

  it('passes through aria-required', () => {
    render(<Textarea aria-required="true" data-testid="test-textarea" />);
    const textarea = screen.getByTestId('test-textarea');
    expect(textarea).toHaveAttribute('aria-required', 'true');
  });

  it('does not render helper/error text (app-owned)', () => {
    const { container } = render(<Textarea data-testid="test-textarea" />);
    expect(container.querySelectorAll('p').length).toBe(0);
    expect(container.querySelectorAll('[role="alert"]').length).toBe(0);
  });

  // --- disabled state ---

  it('applies disabled styling classes when disabled', () => {
    render(<Textarea disabled data-testid="test-textarea" />);
    const textarea = screen.getByTestId('test-textarea');
    expect(textarea).toHaveClass('disabled:bg-gray-100');
    expect(textarea).toHaveClass('disabled:text-gray-700');
    expect(textarea).toHaveClass('disabled:border-gray-alpha-300');
    expect(textarea).toHaveClass('disabled:cursor-not-allowed');
  });
});
