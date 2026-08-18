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
 *   - the section's per-row surface stays read-only (expand toggles only);
 *     the V1.169 P2 create entry is the CardHeader **Add rule** CTA (same
 *     CTA as the empty state) which opens the inline create form.
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
import type { WorldRuleUpdateRequest, WorldRulesListResponse } from '@42ch/nexus-contracts';

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

    // Structural-note copy (PD-1) renders under the header in en.
    expect(screen.getByTestId('world-rules-structural-note')).toHaveTextContent(
      'Each rule carries a structural constraint — module presence/absence, required fields, observer counts — not narrative quality.',
    );
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

  it('collapsed rows stay expand-only; expanded rows expose Edit + Deactivate', async () => {
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

    // The V1.169 P2 create entry (CardHeader CTA) is present.
    expect(screen.getByTestId('world-rules-add-rule')).toHaveTextContent('Add rule');

    // Collapsed rows expose no per-row authoring controls — the row click
    // stays expand-only (T2 lock: Edit lives inside the expanded row).
    expect(screen.queryByTestId('world-rule-edit')).not.toBeInTheDocument();
    expect(screen.queryByTestId('world-rule-deactivate')).not.toBeInTheDocument();

    // Every non-CTA button is a read-only expand/collapse toggle.
    const addRule = screen.getByTestId('world-rules-add-rule');
    const buttons = screen.getAllByRole('button').filter((button) => button !== addRule);
    expect(buttons.length).toBeGreaterThanOrEqual(2);
    for (const button of buttons) {
      expect(button).toHaveAttribute('aria-expanded');
    }

    // Expanding a row reveals the authoring controls (T2).
    fireEvent.click(buttons[0]);
    await waitFor(() => expect(screen.getByTestId('world-rule-edit')).toBeInTheDocument());
    expect(screen.getByTestId('world-rule-deactivate')).toBeInTheDocument();
  });
});

describe('WorldRulesSection — empty state', () => {
  it('renders the locked empty copy with an in-panel Add rule CTA (no CLI pointer)', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () =>
        HttpResponse.json(rulesResponse([], false)),
      ),
    );

    renderSection();
    // Locked copy (plan clarify): title / description / CTA.
    await waitFor(() => expect(screen.getByText('No rules yet')).toBeInTheDocument());
    expect(
      screen.getByText(/Add a structural rule so Check can evaluate this World's entries and timeline\./),
    ).toBeInTheDocument();
    // The CLI pointer is gone (DF-82 blocker).
    expect(screen.queryByText(/creator world rule add/)).not.toBeInTheDocument();
    expect(screen.queryByText(/CLI/)).not.toBeInTheDocument();
    expect(screen.queryByTestId('world-rules-count')).not.toBeInTheDocument();

    // The empty-state CTA opens the inline create form.
    fireEvent.click(screen.getByTestId('world-rules-empty-add-rule'));
    await waitFor(() => expect(screen.getByTestId('world-rule-form')).toBeInTheDocument());
    expect(screen.getByTestId('world-rule-form')).toHaveTextContent('Constraint family');
  });

  it('the CardHeader Add rule CTA also opens the inline form', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () =>
        HttpResponse.json(rulesResponse([], false)),
      ),
    );

    renderSection();
    await waitFor(() => expect(screen.getByText('No rules yet')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('world-rules-add-rule'));
    await waitFor(() => expect(screen.getByTestId('world-rule-form')).toBeInTheDocument());
  });

  it('a successful create refreshes the list and renders the new rule in read order', async () => {
    let list = rulesResponse([], false);
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () => HttpResponse.json(list)),
      http.post('/v1/daemon/worlds/:worldId/rules', async ({ request }) => {
        const body = (await request.json()) as { canonical_name?: string };
        const created = makeRule({
          rule_id: 'rul_created',
          canonical_name: body.canonical_name ?? 'Created rule',
          status: 'active',
        });
        list = rulesResponse([created]);
        return HttpResponse.json(created, { status: 201 });
      }),
    );

    renderSection();
    await waitFor(() => expect(screen.getByText('No rules yet')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('world-rules-empty-add-rule'));
    await waitFor(() => expect(screen.getByTestId('world-rule-form')).toBeInTheDocument());

    // Fill the module_presence happy path.
    fireEvent.change(screen.getByLabelText('Constraint family'), {
      target: { value: 'module_presence' },
    });
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Belief module required' } });
    fireEvent.change(screen.getByLabelText('Summary'), {
      target: { value: 'Entries must carry the belief module.' },
    });
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'belief' } });
    fireEvent.click(screen.getByTestId('rule-form-submit'));

    // The form closes and the refreshed list shows the new row.
    await waitFor(() => expect(screen.queryByTestId('world-rule-form')).not.toBeInTheDocument());
    expect(await screen.findByText('Belief module required')).toBeInTheDocument();
    expect(screen.getByTestId('world-rules-count')).toHaveTextContent('1');
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

describe('WorldRulesSection — loading', () => {
  it('renders LoadingState while the rules request is in flight', async () => {
    // Hold the MSW response open until the assertion has observed the
    // in-flight state (deferred-promise resolver, deterministic — no timing).
    let resolveRules!: (value: WorldRulesListResponse) => void;
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', async () => {
        await new Promise<WorldRulesListResponse>((resolve) => {
          resolveRules = resolve;
        });
        return HttpResponse.json(
          rulesResponse([makeRule({ rule_id: 'rul_load', canonical_name: 'Loaded rule' })]),
        );
      }),
    );

    renderSection();
    // In-flight request → LoadingState; neither the count nor the rows flash.
    expect(screen.getByText('Loading rules…')).toBeInTheDocument();
    expect(screen.queryByTestId('world-rules-count')).not.toBeInTheDocument();

    // Wait until the request has reached the handler, then resolve it — the
    // deferred promise only exists once MSW invokes the resolver.
    await waitFor(() => expect(resolveRules).toBeTypeOf('function'));
    resolveRules(rulesResponse([makeRule({ rule_id: 'rul_load', canonical_name: 'Loaded rule' })]));
    await waitFor(() => expect(screen.getByText('Loaded rule')).toBeInTheDocument());
  });
});

describe('WorldRulesSection — error + retry', () => {
  it('renders ErrorState on failure and retry refetches the list', async () => {
    let failNext = true;
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () => {
        if (failNext) {
          failNext = false;
          return HttpResponse.json(
            { error: { code: 'internal', message: 'boom' } },
            { status: 500 },
          );
        }
        return HttpResponse.json(
          rulesResponse([makeRule({ rule_id: 'rul_retry', canonical_name: 'Retried rule' })]),
        );
      }),
    );

    renderSection();
    // 500 → ErrorState with the retry affordance; no rows/count render.
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Could not load rules');
    expect(screen.queryByTestId('world-rules-count')).not.toBeInTheDocument();

    // Retry re-queries → the second attempt resolves → the list renders.
    fireEvent.click(within(alert).getByRole('button'));
    await waitFor(() => expect(screen.getByText('Retried rule')).toBeInTheDocument());
    expect(screen.getByTestId('world-rules-count')).toHaveTextContent('1');
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
    await waitFor(() => expect(screen.getByText('还没有规则')).toBeInTheDocument());
    expect(screen.getByText('规则')).toBeInTheDocument();
    expect(
      screen.getByText(/结构性约束（模块存在\/缺失、必填字段、观察者数量）/),
    ).toBeInTheDocument();

    // Clean up the persisted preference so later tests render in en again
    // (setup.ts only resets the i18next language, not localStorage).
    window.localStorage.removeItem('nexus-web-locale');
  });
});

describe('WorldRulesSection — edit + Deactivate (V1.169 P2 T2)', () => {
  it('Edit opens the inline edit form prefilled from the stored rule', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () =>
        HttpResponse.json(
          rulesResponse([
            makeRule({
              rule_id: 'rul_edit',
              canonical_name: 'Belief module required',
              statement: 'Entries must carry the belief module.',
              constraint: { family: 'module_presence', module_key: 'belief' },
            }),
          ]),
        ),
      ),
    );

    renderSection();
    const row = await screen.findByTestId('world-rule-row');
    fireEvent.click(row);
    fireEvent.click(await screen.findByTestId('world-rule-edit'));

    const form = await screen.findByTestId('world-rule-form');
    expect(form).toHaveTextContent('Edit rule');
    expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe(
      'Belief module required',
    );
    expect((screen.getByLabelText('Constraint family') as HTMLSelectElement).value).toBe(
      'module_presence',
    );
    expect((screen.getByLabelText('Module key') as HTMLInputElement).value).toBe('belief');
  });

  it('opening edit cancels create and vice versa (one form at a time)', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () =>
        HttpResponse.json(
          rulesResponse([makeRule({ rule_id: 'rul_edit', canonical_name: 'Editable rule' })]),
        ),
      ),
    );

    renderSection();
    await screen.findByTestId('world-rule-row');

    // Create first, then edit: the create form closes.
    fireEvent.click(screen.getByTestId('world-rules-add-rule'));
    await waitFor(() => expect(screen.getByTestId('world-rule-form')).toHaveTextContent('Add rule'));
    const row = screen.getByTestId('world-rule-row');
    fireEvent.click(row);
    fireEvent.click(await screen.findByTestId('world-rule-edit'));
    await waitFor(() => expect(screen.getByTestId('world-rule-form')).toHaveTextContent('Edit rule'));

    // Edit first, then create: the edit form closes.
    fireEvent.click(screen.getByTestId('world-rules-add-rule'));
    await waitFor(() => expect(screen.getByTestId('world-rule-form')).toHaveTextContent('Add rule'));
  });

  it('Deactivate PATCHes status=deprecated; the row stays visible with the deprecated badge', async () => {
    let list = rulesResponse([
      makeRule({ rule_id: 'rul_dep', canonical_name: 'Doomed rule', status: 'active' }),
    ]);
    const patch: { body?: WorldRuleUpdateRequest } = {};
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () => HttpResponse.json(list)),
      http.patch('/v1/daemon/worlds/:worldId/rules/:ruleId', async ({ request }) => {
        const body = (await request.json()) as WorldRuleUpdateRequest;
        patch.body = body;
        list = rulesResponse([
          makeRule({ rule_id: 'rul_dep', canonical_name: 'Doomed rule', status: 'deprecated' }),
        ]);
        return HttpResponse.json(
          makeRule({ rule_id: 'rul_dep', canonical_name: 'Doomed rule', status: 'deprecated' }),
          { status: 200 },
        );
      }),
    );

    renderSection();
    const row = await screen.findByTestId('world-rule-row');
    fireEvent.click(row);
    // Locked Deactivate helper copy (plan clarify, verbatim).
    expect(screen.getByTestId('world-rule-deactivate-help')).toHaveTextContent(
      'Stops Check from auto-including this rule. The rule stays in the list.',
    );
    fireEvent.click(screen.getByTestId('world-rule-deactivate'));

    await waitFor(() => expect(patch.body).toEqual({ status: 'deprecated' }));
    // The row stays visible with the deprecated badge; Deactivate is gone.
    expect(await screen.findByText('Doomed rule')).toBeInTheDocument();
    expect(screen.getByTestId('world-rule-status')).toHaveTextContent('deprecated');
    expect(screen.queryByTestId('world-rule-deactivate')).not.toBeInTheDocument();
  });

  it('reactivates a deprecated rule via Edit → status active', async () => {
    let list = rulesResponse([
      makeRule({
        rule_id: 'rul_react',
        canonical_name: 'Retired rule',
        status: 'deprecated',
        constraint: { family: 'module_presence', module_key: 'belief' },
      }),
    ]);
    const patch: { body?: WorldRuleUpdateRequest } = {};
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () => HttpResponse.json(list)),
      http.patch('/v1/daemon/worlds/:worldId/rules/:ruleId', async ({ request }) => {
        const body = (await request.json()) as WorldRuleUpdateRequest;
        patch.body = body;
        list = rulesResponse([
          makeRule({
            rule_id: 'rul_react',
            canonical_name: 'Retired rule',
            status: 'active',
            constraint: { family: 'module_presence', module_key: 'belief' },
          }),
        ]);
        return HttpResponse.json(
          makeRule({
            rule_id: 'rul_react',
            canonical_name: 'Retired rule',
            status: 'active',
            constraint: { family: 'module_presence', module_key: 'belief' },
          }),
          { status: 200 },
        );
      }),
    );

    renderSection();
    const row = await screen.findByTestId('world-rule-row');
    fireEvent.click(row);
    fireEvent.click(await screen.findByTestId('world-rule-edit'));

    await screen.findByTestId('world-rule-form');
    expect((screen.getByLabelText('Status') as HTMLSelectElement).value).toBe('deprecated');
    fireEvent.change(screen.getByLabelText('Status'), { target: { value: 'active' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));

    await waitFor(() => expect(patch.body?.status).toBe('active'));
    // The form closes and the refreshed list shows the active badge.
    await waitFor(() => expect(screen.queryByTestId('world-rule-form')).not.toBeInTheDocument());
    expect(await screen.findByTestId('world-rule-status')).toHaveTextContent('active');
  });

  it('Deactivate while its edit form is open closes the form so save cannot silently reactivate', async () => {
    let list = rulesResponse([
      makeRule({
        rule_id: 'rul_combo',
        canonical_name: 'Combo rule',
        status: 'active',
        constraint: { family: 'module_presence', module_key: 'belief' },
      }),
    ]);
    const patches: WorldRuleUpdateRequest[] = [];
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () => HttpResponse.json(list)),
      http.patch('/v1/daemon/worlds/:worldId/rules/:ruleId', async ({ request }) => {
        const body = (await request.json()) as WorldRuleUpdateRequest;
        patches.push(body);
        list = rulesResponse([
          makeRule({
            rule_id: 'rul_combo',
            canonical_name: 'Combo rule',
            status: 'deprecated',
            constraint: { family: 'module_presence', module_key: 'belief' },
          }),
        ]);
        return HttpResponse.json(
          makeRule({
            rule_id: 'rul_combo',
            canonical_name: 'Combo rule',
            status: 'deprecated',
            constraint: { family: 'module_presence', module_key: 'belief' },
          }),
          { status: 200 },
        );
      }),
    );

    renderSection();
    const row = await screen.findByTestId('world-rule-row');
    fireEvent.click(row);
    fireEvent.click(await screen.findByTestId('world-rule-edit'));
    await screen.findByTestId('world-rule-form');
    expect((screen.getByLabelText('Status') as HTMLSelectElement).value).toBe('active');

    // Deactivate the rule whose edit form is open.
    fireEvent.click(screen.getByTestId('world-rule-deactivate'));

    // The form closes once deactivation succeeds: the stale form state
    // (status: 'active') can never be saved to silently reactivate the rule.
    await waitFor(() => expect(screen.queryByTestId('world-rule-form')).not.toBeInTheDocument());
    expect(patches).toEqual([{ status: 'deprecated' }]);
    expect(screen.getByTestId('world-rule-status')).toHaveTextContent('deprecated');
  });

  it('list refresh after PATCH keeps the read-route ordering contract', async () => {
    let list = rulesResponse([
      makeRule({
        rule_id: 'rul_a',
        canonical_name: 'Alpha rule',
        constraint: { family: 'module_presence', module_key: 'belief' },
      }),
      makeRule({
        rule_id: 'rul_b',
        canonical_name: 'Beta rule',
        constraint: { family: 'module_absence', module_key: 'tone' },
      }),
    ]);
    const patch: { body?: WorldRuleUpdateRequest } = {};
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/rules', () => HttpResponse.json(list)),
      http.patch('/v1/daemon/worlds/:worldId/rules/:ruleId', async ({ request }) => {
        const body = (await request.json()) as WorldRuleUpdateRequest;
        patch.body = body;
        // The read route re-sorts: the renamed rule moves to its new position.
        list = rulesResponse([
          makeRule({
            rule_id: 'rul_b',
            canonical_name: 'Beta rule',
            constraint: { family: 'module_absence', module_key: 'tone' },
          }),
          makeRule({
            rule_id: 'rul_a',
            canonical_name: 'Zulu rule',
            constraint: { family: 'module_presence', module_key: 'belief' },
          }),
        ]);
        return HttpResponse.json(
          makeRule({
            rule_id: 'rul_a',
            canonical_name: 'Zulu rule',
            constraint: { family: 'module_presence', module_key: 'belief' },
          }),
          { status: 200 },
        );
      }),
    );

    renderSection();
    const rows = await screen.findAllByTestId('world-rule-row');
    fireEvent.click(rows[0]); // Alpha
    fireEvent.click(await screen.findByTestId('world-rule-edit'));
    fireEvent.change(await screen.findByLabelText('Name'), { target: { value: 'Zulu rule' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));

    await waitFor(() => expect(patch.body?.canonical_name).toBe('Zulu rule'));
    await waitFor(() => expect(screen.queryByTestId('world-rule-form')).not.toBeInTheDocument());
    const refreshed = await screen.findAllByTestId('world-rule-row');
    expect(within(refreshed[0]).getByText('Beta rule')).toBeInTheDocument();
    expect(within(refreshed[1]).getByText('Zulu rule')).toBeInTheDocument();
  });
});
