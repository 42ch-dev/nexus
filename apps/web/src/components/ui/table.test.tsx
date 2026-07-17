import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';

import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './table';

/**
 * V1.121 P1 T2 — Table v0.4 recipe (DESIGN.md components.table).
 *
 * Pins the row hover recipe (background-200) and the token-driven motion
 * (duration-state + ease-standard, reduced-motion safe). Structure and ARIA
 * semantics are unchanged.
 */
describe('Table (v0.4 recipes)', () => {
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
});
