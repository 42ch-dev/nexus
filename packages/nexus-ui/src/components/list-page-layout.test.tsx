import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { ListPageLayout } from './list-page-layout';

/**
 * V1.150 P1 — L2 pattern component (DESIGN.md components.list-page-layout).
 *
 * Pins caller-visible behavior: caller-owned copy (title/description) and
 * slot content render, slots appear in page order (relative document order,
 * not wrapper topology), absent slots leave no trace of their content, the
 * actions slot stays interactive for the caller, and the card chrome
 * inherits the promoted Card token recipe (light + dark parity).
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

    // Relative document order only — no assumptions about direct parents or counts.
    const order = [
      screen.getByTestId('filters'),
      screen.getByTestId('results'),
      screen.getByTestId('footer'),
    ];
    for (let i = 0; i < order.length - 1; i += 1) {
      expect(order[i].compareDocumentPosition(order[i + 1]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    }
  });

  it('omits filters and footer content when not passed', () => {
    render(
      <ListPageLayout title="Sessions">
        <ul data-testid="results" />
      </ListPageLayout>,
    );

    expect(screen.getByRole('heading', { name: 'Sessions' })).toBeInTheDocument();
    expect(screen.queryByText('filters')).toBeNull();
    expect(screen.queryByText('load more')).toBeNull();
  });

  it('omits the description text when not passed', () => {
    render(<ListPageLayout title="Works">results</ListPageLayout>);

    expect(screen.queryByText('Review findings for the selected work.')).toBeNull();
  });

  it('wires the actions slot to the caller: click reaches the caller handler', () => {
    const onRefresh = vi.fn();
    render(
      <ListPageLayout title="Findings" actions={<button type="button" onClick={onRefresh}>Refresh</button>}>
        results
      </ListPageLayout>,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    expect(onRefresh).toHaveBeenCalledTimes(1);
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
