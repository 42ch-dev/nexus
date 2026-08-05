/**
 * AssemblyInspectorPanel — P1 T2 (DF-76) coverage.
 *
 * Fixture `MomentInspectResponse` covering the slot map (emit-order
 * grouping), the budget block (estimates + nullable cap/remaining), and the
 * directive-status block (populated directive + the `status: "none"` case).
 * The panel is read-only — these tests assert rendering only, never writes.
 */
import { screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { AssemblyInspectorPanel } from '@/components/inspector/assembly-inspector-panel';
import { renderInApp } from '@/test/test-providers';
import type { MomentInspectResponse } from '@42ch/nexus-contracts';

function makePacket(over: Partial<MomentInspectResponse> = {}): MomentInspectResponse {
  return {
    modules: { placement: [], activation_trace: [] },
    slot_map: [],
    budget: { primary_tokens_est: 0, hop_tokens_est: 0, cap: null, remaining: null },
    moment_directive: {
      scope: null,
      scope_id: null,
      insert_depth: null,
      ttl_kind: null,
      ttl_remaining: null,
      clear_on_scene_change: false,
      status: 'none',
    },
    ...over,
  };
}

describe('AssemblyInspectorPanel — slot map (T2)', () => {
  it('groups/sorts slot_map entries in emit order (world.before → kb.outlet → moment.directive)', () => {
    const packet = makePacket({
      slot_map: [
        { entry_id: 'style-entry', slot: 'style.post_history' },
        { entry_id: 'directive-entry', slot: 'moment.directive' },
        { entry_id: 'outlet-entry', slot: 'kb.outlet.grimoire' },
        { entry_id: 'before-entry', slot: 'world.before' },
        { entry_id: 'default-entry', slot: 'default' },
      ],
    });

    renderInApp(<AssemblyInspectorPanel packet={packet} />);

    const block = screen.getByTestId('slot-map-block');
    const rows = within(block).getAllByTestId(/^slot-row-/);
    // Emit order: world.before, default, kb.outlet.*, style.post_history, moment.directive.
    expect(rows.map((row) => row.getAttribute('data-testid'))).toEqual([
      'slot-row-before-entry',
      'slot-row-default-entry',
      'slot-row-outlet-entry',
      'slot-row-style-entry',
      'slot-row-directive-entry',
    ]);
    expect(within(block).getByTestId('slot-name-before-entry')).toHaveTextContent('world.before');
    expect(within(block).getByTestId('slot-name-outlet-entry')).toHaveTextContent('kb.outlet.grimoire');
  });

  it('renders the empty copy when no slots were captured', () => {
    renderInApp(<AssemblyInspectorPanel packet={makePacket()} />);
    expect(screen.getByTestId('slot-map-empty')).toHaveTextContent('No slots captured');
  });
});

describe('AssemblyInspectorPanel — budget (T2)', () => {
  it('renders estimates and nullable cap/remaining as —', () => {
    renderInApp(
      <AssemblyInspectorPanel
        packet={makePacket({
          budget: { primary_tokens_est: 120, hop_tokens_est: 8, cap: null, remaining: null },
        })}
      />,
    );

    const block = screen.getByTestId('budget-block');
    expect(within(block).getByTestId('budget-primary')).toHaveTextContent('120');
    expect(within(block).getByTestId('budget-hop')).toHaveTextContent('8');
    expect(within(block).getByTestId('budget-cap')).toHaveTextContent('—');
    expect(within(block).getByTestId('budget-remaining')).toHaveTextContent('—');
  });

  it('renders numeric cap/remaining when present', () => {
    renderInApp(
      <AssemblyInspectorPanel
        packet={makePacket({
          budget: { primary_tokens_est: 120, hop_tokens_est: 8, cap: 512, remaining: 384 },
        })}
      />,
    );

    const block = screen.getByTestId('budget-block');
    expect(within(block).getByTestId('budget-cap')).toHaveTextContent('512');
    expect(within(block).getByTestId('budget-remaining')).toHaveTextContent('384');
  });
});

describe('AssemblyInspectorPanel — moment directive status (T2)', () => {
  it('renders directive metadata (status-only — never a body) when active', () => {
    renderInApp(
      <AssemblyInspectorPanel
        packet={makePacket({
          moment_directive: {
            scope: 'work',
            scope_id: 'work-a',
            insert_depth: 'head',
            ttl_kind: 'generations',
            ttl_remaining: 2,
            clear_on_scene_change: true,
            status: 'active',
          },
        })}
      />,
    );

    const block = screen.getByTestId('directive-status-block');
    expect(within(block).getByTestId('directive-status-active')).toHaveTextContent('Active');
    expect(within(block).getByTestId('directive-scope')).toHaveTextContent('work');
    expect(within(block).getByTestId('directive-scope-id')).toHaveTextContent('work-a');
    expect(within(block).getByTestId('directive-depth')).toHaveTextContent('head');
    expect(within(block).getByTestId('directive-ttl')).toHaveTextContent('generations · 2 remaining');
    expect(within(block).getByTestId('directive-clear-on-scene-change')).toHaveTextContent('Yes');
  });

  it('renders "No active directive" for status none', () => {
    renderInApp(<AssemblyInspectorPanel packet={makePacket()} />);

    const block = screen.getByTestId('directive-status-block');
    expect(within(block).getByTestId('directive-none')).toHaveTextContent('No active directive');
    expect(within(block).queryByTestId('directive-status')).toBeNull();
  });

  it('renders the read-only note (the panel observes, never modifies)', () => {
    renderInApp(<AssemblyInspectorPanel packet={makePacket()} />);
    expect(screen.getByTestId('inspector-readonly-note')).toHaveTextContent('Read-only view');
  });
});
