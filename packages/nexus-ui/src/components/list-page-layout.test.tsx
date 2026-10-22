import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { ListPageLayout } from './list-page-layout';

/**
 * V1.150 P1 — L2 pattern component (DESIGN.md components.list-page-layout).
 *
 * Pins: caller-owned copy (title/description) and slots (filters/actions/
 * children/footer) all render in page order, optional slots stay absent when
 * not passed, and the card chrome inherits the promoted Card token recipe
 * (light + dark parity through the tokens).
 */
describe('ListPageLayout', () => {
  it('renders caller copy and every slot in page order', () => {
    render(
      <ListPageLayout
        title="Findings"
        description="Review findings for the selected work."
        filters={<div data-testid="filters">filters</div>}
        actions={<button type="button">Refresh</button>}
        footer={<div data-testid="footer">load more</div>}
      >
        <ul data-testid="results" />
      </ListPageLayout>,
    );

    expect(screen.getByRole('heading', { name: 'Findings' })).toBeInTheDocument();
    expect(screen.getByText('Review findings for the selected work.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Refresh' })).toBeInTheDocument();

    const content = screen.getByTestId('results').parentElement!;
    const contentChildren = Array.from(content.children);
    const expected = [
      screen.getByTestId('filters').parentElement!,
      screen.getByTestId('results'),
      screen.getByTestId('footer').parentElement!,
    ];
    expected.forEach((el, i) => {
      expect(contentChildren[i]).toBe(el);
    });
  });

  it('omits the filters row and footer slot when not passed', () => {
    render(
      <ListPageLayout title="Sessions">
        <ul data-testid="results" />
      </ListPageLayout>,
    );

    expect(screen.getByRole('heading', { name: 'Sessions' })).toBeInTheDocument();
    const content = screen.getByTestId('results').parentElement!;
    expect(content.children).toHaveLength(1);
  });

  it('omits the description element when not passed', () => {
    const { container } = render(<ListPageLayout title="Works">results</ListPageLayout>);

    expect(container.querySelector('p')).toBeNull();
  });

  it('page card inherits the promoted Card token recipe (light/dark parity)', () => {
    const { container } = render(<ListPageLayout title="Works">results</ListPageLayout>);

    const card = container.firstElementChild!;
    expect(card.className).toMatch(/\bbg-background-100\b/);
    expect(card.className).toMatch(/\bborder-gray-alpha-400\b/);
    expect(card.className).toMatch(/\bshadow-card\b/);
  });

  it('merges a caller className onto the page card', () => {
    const { container } = render(
      <ListPageLayout title="Works" className="findings-page">
        results
      </ListPageLayout>,
    );

    expect(container.firstElementChild!.className).toMatch(/\bfindings-page\b/);
  });
});
