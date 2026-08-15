/**
 * World Rules section tests — V1.166 P2 T2 (DR-64 surfacing half).
 *
 * Covers the three section states (populated / empty / truncated) plus the
 * spoke-vocabulary contract (PD-1/PD-2 locked):
 *   - status renders `draft|active|deprecated` verbatim with an open-string
 *     fallback — never remapped to a different lifecycle vocabulary;
 *   - `kind` / `severity_hint` render verbatim (severity reuses T1's token
 *     mapping);
 *   - constraint-carrier summaries render defensively from the DTO's
 *     first-class `constraint` object (AR-2/AR-3): all four families +
 *     module-row form, unknown families get family + generic operands, odd
 *     shapes never crash, absent constraint → no summary row;
 *   - the section is read-only — no create/edit/deactivate controls.
 */
import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';

import { WorldRulesSection } from '@/components/worlds/world-rules/world-rules-section';
import { renderConstraintSummary } from '@/components/worlds/world-rules/constraint-summary';
import { worldRuleStatusVariant } from '@/components/worlds/world-rules/world-rule-status-badge';
import { BrowserClient } from '@/lib/nexus';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import type { WorldRulesListResponse } from '@42ch/nexus-contracts';

type WorldRule = WorldRulesListResponse['rules'][number];

const WORLD_ID = 'world-9';

function makeRule(over: Partial<WorldRule> = {}): WorldRule {
  return {
    rule_id: 'rul_00000000000000000000000000000001',
    canonical_name: 'Characters need summaries',
    kind: 'rule',
    target_entry_types: [],
    status: 'active',
    severity_hint: 'warning',
    statement: 'Every character entry must carry a body summary.',
    ...over,
  };
}

function rulesResponse(rules: WorldRule[], truncated = false): WorldRulesListResponse {
  return { rules, truncated };
}

function renderSection(worldId = WORLD_ID) {
  return renderInApp(<WorldRulesSection worldId={worldId} />, {
    client: new BrowserClient(),
  });
}

describe('worldRuleStatusVariant — spoke vocabulary with open-string fallback', () => {
  it('maps the three spoke statuses to design-token tones', () => {
    expect(worldRuleStatusVariant('active')).toBe('running'); // live, auto-included → green
    expect(worldRuleStatusVariant('deprecated')).toBe('warning'); // stale → amber
    expect(worldRuleStatusVariant('draft')).toBe('neutral'); // staged, not evaluated
  });

  it('is case-insensitive for the spoke vocabulary', () => {
    expect(worldRuleStatusVariant('ACTIVE')).toBe('running');
    expect(worldRuleStatusVariant('Draft')).toBe('neutral');
    expect(worldRuleStatusVariant('Deprecated')).toBe('warning');
  });

  it('falls back to neutral for open strings (no lifecycle remap)', () => {
    expect(worldRuleStatusVariant('retired')).toBe('neutral');
    expect(worldRuleStatusVariant('paused')).toBe('neutral');
    expect(worldRuleStatusVariant('inactive')).toBe('neutral');
    expect(worldRuleStatusVariant(undefined)).toBe('neutral');
    expect(worldRuleStatusVariant(null)).toBe('neutral');
  });
});

describe('renderConstraintSummary — defensive carrier rendering (AR-2/AR-3)', () => {
  it('renders module_presence / module_absence with the module key', () => {
    expect(renderConstraintSummary({ family: 'module_presence', module_key: 'belief' })).toBe(
      'module_presence: belief',
    );
    expect(renderConstraintSummary({ family: 'module_absence', module_key: 'tone' })).toBe(
      'module_absence: tone',
    );
  });

  it('renders required_field entry-level and module-row forms', () => {
    expect(renderConstraintSummary({ family: 'required_field', field: 'body.summary' })).toBe(
      'required_field: body.summary',
    );
    expect(
      renderConstraintSummary({ family: 'required_field', module_key: 'journal', field: 'tags' }),
    ).toBe('required_field: journal.tags');
  });

  it('renders observer_cardinality bounds (both / partial)', () => {
    expect(renderConstraintSummary({ family: 'observer_cardinality', min: 0, max: 3 })).toBe(
      'observer_cardinality: min 0 · max 3',
    );
    expect(renderConstraintSummary({ family: 'observer_cardinality', min: 1 })).toBe(
      'observer_cardinality: min 1',
    );
    expect(renderConstraintSummary({ family: 'observer_cardinality', max: 3 })).toBe(
      'observer_cardinality: max 3',
    );
  });

  it('returns null when the constraint is absent or malformed (no summary row)', () => {
    expect(renderConstraintSummary(undefined)).toBeNull();
    expect(renderConstraintSummary({})).toBeNull();
    expect(renderConstraintSummary({ family: 42 })).toBeNull();
    expect(renderConstraintSummary({ family: '' })).toBeNull();
  });

  it('renders unknown families with generic operands — never crashes', () => {
    expect(
      renderConstraintSummary({ family: 'custom_family', mode: 'strict', weight: 2 }),
    ).toBe('custom_family: mode: strict · weight: 2');
    expect(renderConstraintSummary({ family: 'mystery' })).toBe('mystery');
  });

  it('tolerates odd shapes: non-scalar operands skipped, partial fields honest', () => {
    // Non-scalar members are skipped in the generic path (no crash, no [object Object]).
    expect(
      renderConstraintSummary({ family: 'custom_family', nested: { a: 1 }, list: [1, 2] }),
    ).toBe('custom_family');
    // String-typed numeric bounds fall through to the generic operand path.
    expect(renderConstraintSummary({ family: 'observer_cardinality', min: '3' })).toBe(
      'observer_cardinality: min: 3',
    );
    // required_field with only the module key renders the module row honestly.
    expect(renderConstraintSummary({ family: 'required_field', module_key: 'journal' })).toBe(
      'required_field: journal',
    );
    // Known family with no usable operands falls back to the family string.
    expect(renderConstraintSummary({ family: 'module_presence' })).toBe('module_presence');
  });
});

describe('WorldRulesSection — populated', () => {
  it('renders all statuses verbatim (active/draft/deprecated + open string, no remap)', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () =>
        HttpResponse.json(
          rulesResponse([
            makeRule({ rule_id: 'rul_active', canonical_name: 'Live rule', status: 'active' }),
            makeRule({ rule_id: 'rul_draft', canonical_name: 'Staged rule', status: 'draft' }),
            makeRule({ rule_id: 'rul_dep', canonical_name: 'Retired rule', status: 'deprecated' }),
            makeRule({
              rule_id: 'rul_open',
              canonical_name: 'Odd rule',
              status: 'paused',
              kind: 'prohibition',
            }),
          ]),
        ),
      ),
    );

    renderSection();
    await waitFor(() => expect(screen.getByText('Live rule')).toBeInTheDocument());

    // Status strings render verbatim — never coerced to another vocabulary.
    expect(screen.getByText('active')).toBeInTheDocument();
    expect(screen.getByText('draft')).toBeInTheDocument();
    expect(screen.getByText('deprecated')).toBeInTheDocument();
    expect(screen.getByText('paused')).toBeInTheDocument();

    // Open-string `kind` renders verbatim too.
    expect(screen.getByText('prohibition')).toBeInTheDocument();

    // Count reflects the returned rows.
    expect(screen.getByTestId('world-rules-count')).toHaveTextContent('4');
  });

  it('expands a rule to show statement, constraint summary, and target entry types', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () =>
        HttpResponse.json(
          rulesResponse([
            makeRule({
              rule_id: 'rul_expand',
              canonical_name: 'Observer bound',
              status: 'active',
              severity_hint: 'error',
              statement: 'Timeline events must record 0-3 observers.',
              target_entry_types: ['timeline_event'],
              constraint: { family: 'observer_cardinality', min: 0, max: 3 },
            }),
          ]),
        ),
      ),
    );

    renderSection();
    const row = await screen.findByTestId('world-rule-row');
    // Severity hint reuses T1's token mapping and renders verbatim.
    expect(within(row).getByText('error')).toBeInTheDocument();

    fireEvent.click(row);
    await waitFor(() => expect(screen.getByTestId('world-rule-detail')).toBeInTheDocument());
    expect(screen.getByTestId('world-rule-detail')).toHaveTextContent(
      'Timeline events must record 0-3 observers.',
    );
    expect(screen.getByTestId('world-rule-constraint')).toHaveTextContent(
      'observer_cardinality: min 0 · max 3',
    );
    expect(screen.getByTestId('world-rule-target-types')).toHaveTextContent('timeline_event');

    // Expand toggles back.
    fireEvent.click(row);
    await waitFor(() => expect(screen.queryByTestId('world-rule-detail')).not.toBeInTheDocument());
  });

  it('renders constraint summaries for all four families + module-row form + absent constraint', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () =>
        HttpResponse.json(
          rulesResponse([
            makeRule({
              rule_id: 'rul_mp',
              canonical_name: 'Belief module required',
              constraint: { family: 'module_presence', module_key: 'belief' },
            }),
            makeRule({
              rule_id: 'rul_ma',
              canonical_name: 'Tone module forbidden',
              constraint: { family: 'module_absence', module_key: 'tone' },
            }),
            makeRule({
              rule_id: 'rul_rf',
              canonical_name: 'Summary required',
              constraint: { family: 'required_field', field: 'body.summary' },
            }),
            makeRule({
              rule_id: 'rul_rfm',
              canonical_name: 'Journal tags required',
              constraint: { family: 'required_field', module_key: 'journal', field: 'tags' },
            }),
            makeRule({
              rule_id: 'rul_oc',
              canonical_name: 'Observer cap',
              constraint: { family: 'observer_cardinality', min: 0, max: 3 },
            }),
            makeRule({ rule_id: 'rul_none', canonical_name: 'No carrier' }),
          ]),
        ),
      ),
    );

    renderSection();
    const rows = await screen.findAllByTestId('world-rule-row');

    fireEvent.click(rows[0]);
    expect(await screen.findByTestId('world-rule-constraint')).toHaveTextContent(
      'module_presence: belief',
    );
    fireEvent.click(rows[0]);

    fireEvent.click(rows[1]);
    await waitFor(() =>
      expect(screen.getByTestId('world-rule-constraint')).toHaveTextContent('module_absence: tone'),
    );
    fireEvent.click(rows[1]);

    fireEvent.click(rows[2]);
    await waitFor(() =>
      expect(screen.getByTestId('world-rule-constraint')).toHaveTextContent(
        'required_field: body.summary',
      ),
    );
    fireEvent.click(rows[2]);

    fireEvent.click(rows[3]);
    await waitFor(() =>
      expect(screen.getByTestId('world-rule-constraint')).toHaveTextContent(
        'required_field: journal.tags',
      ),
    );
    fireEvent.click(rows[3]);

    fireEvent.click(rows[4]);
    await waitFor(() =>
      expect(screen.getByTestId('world-rule-constraint')).toHaveTextContent(
        'observer_cardinality: min 0 · max 3',
      ),
    );
    fireEvent.click(rows[4]);

    // Absent constraint → no summary row for the last rule.
    fireEvent.click(rows[5]);
    const detail = await screen.findByTestId('world-rule-detail');
    expect(within(detail).queryByTestId('world-rule-constraint')).not.toBeInTheDocument();
    // Empty target_entry_types renders the honest "all entry types" copy.
    expect(within(detail).getByTestId('world-rule-target-types')).toHaveTextContent(
      'All entry types',
    );
    fireEvent.click(rows[5]);
  });

  it('is read-only: no create/edit/deactivate controls, only expand toggles', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () =>
        HttpResponse.json(
          rulesResponse([
            makeRule({ rule_id: 'rul_ro', canonical_name: 'Read-only rule' }),
            makeRule({ rule_id: 'rul_ro2', canonical_name: 'Read-only rule 2' }),
          ]),
        ),
      ),
    );

    renderSection();
    await waitFor(() => expect(screen.getByText('Read-only rule')).toBeInTheDocument());

    // No authoring controls (PD-2 — rules are CLI-authored, PD-1).
    expect(
      screen.queryByRole('button', { name: /create|edit|deactivate|add|delete/i }),
    ).not.toBeInTheDocument();

    // Every button is a read-only expand/collapse toggle.
    const buttons = screen.getAllByRole('button');
    expect(buttons.length).toBeGreaterThanOrEqual(2);
    for (const button of buttons) {
      expect(button).toHaveAttribute('aria-expanded');
    }
  });
});

describe('WorldRulesSection — empty state', () => {
  it('renders the honest empty copy when the world has no rules', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () =>
        HttpResponse.json(rulesResponse([], false)),
      ),
    );

    renderSection();
    await waitFor(() => expect(screen.getByText('No rules')).toBeInTheDocument());
    expect(screen.getByText(/This World has no rules yet/)).toBeInTheDocument();
    expect(screen.queryByTestId('world-rules-count')).not.toBeInTheDocument();
  });
});

describe('WorldRulesSection — truncated honesty', () => {
  it('renders the 500-cap copy when truncated is true', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () =>
        HttpResponse.json(
          rulesResponse([makeRule({ rule_id: 'rul_trunc', canonical_name: 'Truncated rule' })], true),
        ),
      ),
    );

    renderSection();
    await waitFor(() => expect(screen.getByText('Truncated rule')).toBeInTheDocument());
    expect(screen.getByTestId('world-rules-truncated')).toHaveTextContent(
      'Showing the first 500 rules by name',
    );
  });

  it('shows no truncation banner when truncated is false', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () =>
        HttpResponse.json(
          rulesResponse([makeRule({ rule_id: 'rul_nt', canonical_name: 'Not truncated' })], false),
        ),
      ),
    );

    renderSection();
    await waitFor(() => expect(screen.getByText('Not truncated')).toBeInTheDocument());
    expect(screen.queryByTestId('world-rules-truncated')).not.toBeInTheDocument();
  });
});

describe('WorldRulesSection — zh-CN locale parity', () => {
  it('renders section chrome in zh-CN when the locale preference is set', async () => {
    window.localStorage.setItem('nexus-web-locale', 'zh-CN');
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () =>
        HttpResponse.json(rulesResponse([], false)),
      ),
    );

    renderSection();
    await waitFor(() => expect(screen.getByText('暂无规则')).toBeInTheDocument());
    expect(screen.getByText('规则')).toBeInTheDocument();
    expect(
      screen.getByText(/结构性约束（模块存在\/缺失、必填字段、观察者数量）/),
    ).toBeInTheDocument();
  });
});
