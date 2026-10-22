import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { ListDetailShell } from './list-detail-shell';

/**
 * V1.150 P3 — L2 pattern (DESIGN.md §Component Primitives/ListDetailShell).
 *
 * Pins caller-visible behavior: caller renders both the master list region
 * and the detail region (panel or a "no selection" EmptyState — the shell
 * owns no open/close state), caller content passes through untouched, and
 * the fixed-width responsive split recipe (stack below lg, 360px detail
 * column at lg) is the promoted token recipe.
 */
describe('ListDetailShell', () => {
  it('renders the caller list and detail regions', () => {
    render(
      <ListDetailShell
        list={<div data-testid="list">master list</div>}
        detail={<div data-testid="detail">detail panel</div>}
      />,
    );

    expect(screen.getByTestId('list')).toBeInTheDocument();
    expect(screen.getByTestId('list')).toHaveTextContent('master list');
    expect(screen.getByTestId('detail')).toBeInTheDocument();
    expect(screen.getByTestId('detail')).toHaveTextContent('detail panel');
  });

  it('lets the caller render a "no selection" state in the detail region', () => {
    render(
      <ListDetailShell
        list={<div data-testid="list">master list</div>}
        detail={<p>No selection</p>}
      />,
    );

    expect(screen.getByText('No selection')).toBeInTheDocument();
  });

  it('keeps caller interactions inside both regions working', () => {
    let clicked = 0;
    render(
      <ListDetailShell
        list={
          <button type="button" onClick={() => (clicked += 1)}>
            select row
          </button>
        }
        detail={<div>detail</div>}
      />,
    );

    screen.getByRole('button', { name: 'select row' }).click();
    expect(clicked).toBe(1);
  });

  it('merges a caller className', () => {
    const { container } = render(
      <ListDetailShell
        list={<div>list</div>}
        detail={<div>detail</div>}
        className="findings-shell"
      />,
    );

    expect(container.firstElementChild!.className).toMatch(/\bfindings-shell\b/);
  });

  it('uses the fixed-width responsive split recipe', () => {
    const { container } = render(
      <ListDetailShell list={<div>list</div>} detail={<div>detail</div>} />,
    );

    const className = container.firstElementChild!.className;
    expect(className).toMatch(/\blg:grid-cols-\[minmax\(0,1fr\)_360px\]/);
    expect(className).toMatch(/\bgrid\b/);
  });
});
