import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Select } from './select';

describe('Select', () => {
  // --- base rendering ---

  it('renders a select element', () => {
    render(
      <Select data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    const select = screen.getByTestId('test-select');
    expect(select).toHaveProperty('tagName', 'SELECT');
  });

  it('renders with base structural classes', () => {
    render(
      <Select data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    const select = screen.getByTestId('test-select');
    expect(select).toHaveClass('h-10');
    expect(select).toHaveClass('w-full');
    expect(select).toHaveClass('rounded-control');
    expect(select).toHaveClass('border');
    expect(select).toHaveClass('bg-background-100');
    expect(select).toHaveClass('appearance-none');
    expect(select).toHaveClass('focus-visible:border-blue-700');
  });

  it('renders option children supplied by the consumer', () => {
    render(
      <Select data-testid="test-select" defaultValue="b">
        <option value="a">Option A</option>
        <option value="b">Option B</option>
      </Select>,
    );
    expect(screen.getByRole('option', { name: 'Option A' })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: 'Option B' })).toBeInTheDocument();
  });

  // --- chevron inset (asymmetric horizontal padding) ---

  it('renders with asymmetric inline padding for chevron inset', () => {
    render(
      <Select data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    const select = screen.getByTestId('test-select');
    expect(select).toHaveClass('ps-3');
    expect(select).toHaveClass('pe-8');
    expect(select).not.toHaveClass('px-3');
  });

  it('inherits chevron inset when disabled', () => {
    render(
      <Select disabled data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    const select = screen.getByTestId('test-select');
    expect(select).toHaveClass('ps-3');
    expect(select).toHaveClass('pe-8');
    expect(select).not.toHaveClass('px-3');
  });

  it('inherits chevron inset when invalid', () => {
    render(
      <Select invalid data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    const select = screen.getByTestId('test-select');
    expect(select).toHaveClass('ps-3');
    expect(select).toHaveClass('pe-8');
    expect(select).not.toHaveClass('px-3');
  });

  // --- custom chevron overlay ---

  it('renders a custom chevron overlay', () => {
    render(
      <Select data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    const chevron = screen.getByTestId('select-chevron');
    expect(chevron).toBeInTheDocument();
    expect(chevron).toHaveAttribute('aria-hidden', 'true');
    expect(chevron).toHaveClass('right-3');
    expect(chevron).toHaveClass('pointer-events-none');
  });

  it('keeps the custom chevron in disabled and invalid states', () => {
    const { rerender } = render(
      <Select disabled data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    expect(screen.getByTestId('select-chevron')).toBeInTheDocument();

    rerender(
      <Select invalid data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    expect(screen.getByTestId('select-chevron')).toBeInTheDocument();
  });

  // --- className merge (cn integration) ---

  it('merges custom className with base classes', () => {
    render(
      <Select className="custom-class" data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    const select = screen.getByTestId('test-select');
    expect(select).toHaveClass('custom-class');
    expect(select).toHaveClass('h-10');
  });

  // --- invalid prop → aria-invalid + visual state ---

  it('sets aria-invalid="true" when invalid is true', () => {
    render(
      <Select invalid data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    expect(screen.getByTestId('test-select')).toHaveAttribute('aria-invalid', 'true');
  });

  it('does not set aria-invalid when invalid is false', () => {
    render(
      <Select invalid={false} data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    expect(screen.getByTestId('test-select')).not.toHaveAttribute('aria-invalid');
  });

  it('does not set aria-invalid when invalid is omitted', () => {
    render(
      <Select data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    expect(screen.getByTestId('test-select')).not.toHaveAttribute('aria-invalid');
  });

  it('applies red-700 border class when invalid', () => {
    render(
      <Select invalid data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    const select = screen.getByTestId('test-select');
    expect(select).toHaveClass('border-red-700');
    expect(select).not.toHaveClass('border-gray-alpha-400');
  });

  it('applies gray-alpha-400 border class when not invalid', () => {
    render(
      <Select data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    const select = screen.getByTestId('test-select');
    expect(select).toHaveClass('border-gray-alpha-400');
    expect(select).not.toHaveClass('border-red-700');
  });

  // --- ref-as-prop ---

  it('passes the ref to the underlying select element', () => {
    let ref: HTMLSelectElement | null = null;
    const setRef = (el: HTMLSelectElement | null) => {
      ref = el;
    };
    render(
      <Select ref={setRef} data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    expect(ref).not.toBeNull();
    expect(ref!).toHaveProperty('tagName', 'SELECT');
    expect(ref).toBe(screen.getByTestId('test-select'));
  });

  // --- accessibility-relevant attributes ---

  it('passes through standard aria-describedby', () => {
    render(
      <Select aria-describedby="helper-1 error-1" data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    expect(screen.getByTestId('test-select')).toHaveAttribute(
      'aria-describedby',
      'helper-1 error-1',
    );
  });

  it('passes through standard id', () => {
    render(
      <Select id="my-field" data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    expect(screen.getByTestId('test-select')).toHaveAttribute('id', 'my-field');
  });

  it('passes through native required attribute', () => {
    render(
      <Select required data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    expect(screen.getByTestId('test-select')).toBeRequired();
  });

  it('does not set aria-expanded (UA owns open state)', () => {
    render(
      <Select data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    expect(screen.getByTestId('test-select')).not.toHaveAttribute('aria-expanded');
  });

  it('does not render helper/error text (app-owned)', () => {
    const { container } = render(
      <Select data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    expect(container.querySelectorAll('p').length).toBe(0);
    expect(container.querySelectorAll('[role="alert"]').length).toBe(0);
  });

  // --- disabled state ---

  it('applies disabled styling classes when disabled', () => {
    render(
      <Select disabled data-testid="test-select">
        <option value="a">A</option>
      </Select>,
    );
    const select = screen.getByTestId('test-select');
    expect(select).toBeDisabled();
    expect(select).toHaveClass('disabled:bg-gray-100');
    expect(select).toHaveClass('disabled:text-gray-700');
    expect(select).toHaveClass('disabled:border-gray-alpha-300');
    expect(select).toHaveClass('disabled:cursor-not-allowed');
  });
});
