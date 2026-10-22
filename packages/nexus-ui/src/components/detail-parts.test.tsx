import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { DetailCard, FieldRow } from './detail-parts';

/**
 * V1.150 P3 — L2 pattern family (DESIGN.md §Component Primitives/detail-parts).
 *
 * Pins caller-visible behavior: FieldRow renders caller label/value copy and
 * hides the row entirely on null/undefined/'' values (shared null-hide
 * behavior); DetailCard renders caller title/description/children over the
 * promoted Card recipe; both merge caller className and keep the family's
 * token recipe.
 */
describe('FieldRow', () => {
  it('renders caller label and value copy', () => {
    render(<FieldRow label="Status" value="Ready" />);

    expect(screen.getByText('Status')).toBeInTheDocument();
    expect(screen.getByText('Ready')).toBeInTheDocument();
  });

  it('renders nothing when value is missing, null, undefined, or empty string', () => {
    const { container: missing } = render(<FieldRow label="Status" />);
    expect(missing.firstElementChild).toBeNull();

    const { container: nullValue } = render(<FieldRow label="Status" value={null} />);
    expect(nullValue.firstElementChild).toBeNull();

    const { container: undefinedValue } = render(<FieldRow label="Status" value={undefined} />);
    expect(undefinedValue.firstElementChild).toBeNull();

    const { container: empty } = render(<FieldRow label="Status" value="" />);
    expect(empty.firstElementChild).toBeNull();
  });

  it('keeps meaningful falsy values (0 and false) rendered', () => {
    const { container: zero } = render(<FieldRow label="Retries" value={0} />);
    expect(zero.firstElementChild).not.toBeNull();
    expect(screen.getByText('0')).toBeInTheDocument();

    const { container: no } = render(<FieldRow label="Enabled" value={false} />);
    expect(no.firstElementChild).not.toBeNull();
    expect(screen.getByText('Enabled')).toBeInTheDocument();
  });

  it('keeps the detail token recipe (light/dark parity)', () => {
    render(<FieldRow label="Status" value="Ready" />);

    expect(screen.getByText('Status').className).toMatch(/\btext-gray-700\b/);
    expect(screen.getByText('Ready').className).toMatch(/\btext-gray-1000\b/);
  });

  it('merges a caller className', () => {
    const { container } = render(<FieldRow label="Status" value="Ready" className="detail-field" />);

    expect(container.firstElementChild!.className).toMatch(/\bdetail-field\b/);
  });
});

describe('DetailCard', () => {
  it('renders caller title, description, and children', () => {
    render(
      <DetailCard title="Run detail" description="Latest run summary.">
        <div data-testid="body">field rows</div>
      </DetailCard>,
    );

    expect(screen.getByText('Run detail')).toBeInTheDocument();
    expect(screen.getByText('Latest run summary.')).toBeInTheDocument();
    expect(screen.getByTestId('body')).toBeInTheDocument();
  });

  it('omits the description when not passed', () => {
    render(
      <DetailCard title="Run detail">
        <div>body</div>
      </DetailCard>,
    );

    expect(screen.getByText('Run detail')).toBeInTheDocument();
    expect(screen.queryByText('Latest run summary.')).toBeNull();
  });

  it('composes the promoted Card chrome recipe', () => {
    const { container } = render(
      <DetailCard title="Run detail">
        <div>body</div>
      </DetailCard>,
    );

    // Card family: rounded border surface; header title + optional description
    // ride the Card token recipe (background-100 fill, gray-alpha-400 border).
    const surface = container.firstElementChild!;
    expect(surface.className).toMatch(/\brounded-card\b/);
    expect(surface.className).toMatch(/\bborder-gray-alpha-400\b/);
    expect(surface.className).toMatch(/\bbg-background-100\b/);
    expect(surface.className).toMatch(/\btext-gray-1000\b/);
  });

  it('merges a caller className', () => {
    const { container } = render(
      <DetailCard title="Run detail" className="inspector-card">
        <div>body</div>
      </DetailCard>,
    );

    expect(container.firstElementChild!.className).toMatch(/\binspector-card\b/);
  });
});
