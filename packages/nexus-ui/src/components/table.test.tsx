import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './table';

/**
 * V1.148 P1 — promoted Table primitives (DESIGN.md components.table).
 *
 * Pins: native table semantics, the v0.4 recipes (header background-200 well,
 * label-12 head, background-200 row hover with token-driven reduced-motion-safe
 * transition), and caller className merging. Theme parity comes from
 * `--nexus-*` token projections, not literals.
 */
function renderTable() {
  return render(
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Name</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        <TableRow>
          <TableCell>Alpha</TableCell>
        </TableRow>
      </TableBody>
    </Table>,
  );
}

describe('Table', () => {
  it('renders native table structure with header, body, row, head, and cell', () => {
    const { container } = renderTable();
    expect(container.querySelector('table')).not.toBeNull();
    expect(container.querySelector('thead')).not.toBeNull();
    expect(container.querySelector('tbody')).not.toBeNull();
    expect(container.querySelector('tbody tr')).toHaveTextContent('Alpha');
  });

  it('wraps in an overflow-x container for narrow screens', () => {
    const { container } = renderTable();
    const wrapper = container.firstChild as HTMLElement;
    expect(wrapper.className).toMatch(/\boverflow-x-auto\b/);
  });

  it('row hover uses background-200 with token-driven, reduced-motion-safe transition', () => {
    const { container } = renderTable();
    const bodyRow = container.querySelector('tbody tr');
    expect(bodyRow).not.toBeNull();
    expect(bodyRow!.className).toMatch(/\bhover:bg-background-200\b/);
    expect(bodyRow!.className).toMatch(/\btransition-colors\b/);
    expect(bodyRow!.className).toMatch(/\bduration-state\b/);
    expect(bodyRow!.className).toMatch(/\bease-standard\b/);
    expect(bodyRow!.className).toMatch(/\bmotion-reduce:transition-none\b/);
  });

  it('header consumes the background-200 well + label-12 recipe', () => {
    const { container } = renderTable();
    const head = container.querySelector('thead');
    const th = container.querySelector('th');
    expect(head!.className).toMatch(/\bbg-background-200\b/);
    expect(head!.className).toMatch(/\btext-gray-900\b/);
    expect(th!.className).toMatch(/\btext-label-12\b/);
    expect(th!.className).toMatch(/\bborder-gray-alpha-400\b/);
  });

  it('merges caller className overrides', () => {
    const { container } = render(
      <Table className="text-label-12">
        <TableBody>
          <TableRow className="hover:bg-background-300">
            <TableCell className="px-6">Alpha</TableCell>
          </TableRow>
        </TableBody>
      </Table>,
    );
    expect(container.querySelector('table')!.className).toMatch(/\btext-label-12\b/);
    expect(container.querySelector('tr')!.className).toMatch(/\bhover:bg-background-300\b/);
    expect(container.querySelector('td')!.className).toMatch(/\bpx-6\b/);
  });
});
