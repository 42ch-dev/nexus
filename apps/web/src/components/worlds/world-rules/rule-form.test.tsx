/**
 * RuleForm tests — V1.169 P2 T1 (DF-82 create form).
 *
 * Covers the four-family happy path + submit payload shape, the client
 * validation mirror (immediate interception; the API's field-level errors
 * stay the SSOT — AR-2), 400 `invalid_input` field-level echo mapped 1:1
 * onto the AR-2 field vocabulary, success close, family-switch operand
 * reset, observer × target-entry-types exclusivity, and the no-raw-JSON
 * rule.
 */
import { fireEvent, screen, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';

import { RuleForm } from '@/components/worlds/world-rules/rule-form';
import {
  buildCreateWorldRuleRequest,
  buildUpdateWorldRuleRequest,
  initialRuleFormState,
  ruleFormStateFromRule,
  validateRuleForm,
  type RuleFormState,
} from '@/components/worlds/world-rules/rule-form-state';
import { BrowserClient } from '@/lib/nexus';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import type {
  WorldRuleCreateRequest,
  WorldRuleResponse,
  WorldRuleUpdateRequest,
  WorldRulesListResponse,
} from '@42ch/nexus-contracts';

type WorldRule = WorldRulesListResponse['rules'][number];

const WORLD_ID = 'world-9';

function ruleResponse(over: Partial<WorldRuleResponse> = {}): WorldRuleResponse {
  return {
    rule_id: 'rul_created',
    canonical_name: 'Created rule',
    kind: 'rule',
    target_entry_types: [],
    status: 'active',
    statement: 'Created statement',
    ...over,
  };
}

function renderForm(onClose = vi.fn()) {
  return renderInApp(<RuleForm worldId={WORLD_ID} onClose={onClose} />, {
    client: new BrowserClient(),
  });
}

/** Capture the POST body; `respond` may return a custom HttpResponse. */
function useCreateCapture(
  capture: { body?: WorldRuleCreateRequest },
  respond?: (body: WorldRuleCreateRequest) => Response,
) {
  useHandlers(
    http.post('/v1/daemon/worlds/:worldId/rules', async ({ request }) => {
      const body = (await request.json()) as WorldRuleCreateRequest;
      capture.body = body;
      return respond ? respond(body) : HttpResponse.json(ruleResponse(), { status: 201 });
    }),
  );
}

function selectFamily(value: RuleFormState['family']) {
  fireEvent.change(screen.getByLabelText('Constraint family'), { target: { value } });
}

function fillNameAndSummary(name: string, summary: string) {
  fireEvent.change(screen.getByLabelText('Name'), { target: { value: name } });
  fireEvent.change(screen.getByLabelText('Summary'), { target: { value: summary } });
}

function submit() {
  fireEvent.click(screen.getByRole('button', { name: 'Add rule' }));
}

/** A stored read-item projection for edit-mode tests (T2). */
function makeStoredRule(over: Partial<WorldRule> = {}): WorldRule {
  return {
    rule_id: 'rul_edit',
    canonical_name: 'Stored rule',
    kind: 'rule',
    target_entry_types: [],
    status: 'active',
    severity_hint: 'warning',
    statement: 'Stored statement',
    constraint: { family: 'module_presence', module_key: 'belief' },
    ...over,
  };
}

/** Capture the PATCH body; `respond` may return a custom HttpResponse. */
function useUpdateCapture(
  capture: { body?: WorldRuleUpdateRequest },
  respond?: (body: WorldRuleUpdateRequest) => Response,
) {
  useHandlers(
    http.patch('/v1/daemon/worlds/:worldId/rules/:ruleId', async ({ request }) => {
      const body = (await request.json()) as WorldRuleUpdateRequest;
      capture.body = body;
      return respond ? respond(body) : HttpResponse.json(ruleResponse(), { status: 200 });
    }),
  );
}

function renderEditForm(rule: WorldRule, onClose = vi.fn()) {
  return renderInApp(<RuleForm worldId={WORLD_ID} rule={rule} onClose={onClose} />, {
    client: new BrowserClient(),
  });
}

function submitEdit() {
  fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));
}

describe('RuleForm — four-family happy path + submit payload shape', () => {
  it('module_presence: locked family copy, locked helpers, default-status carrier', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    const onClose = vi.fn();
    renderForm(onClose);

    selectFamily('module_presence');
    // Locked one-line family hint (plan table, verbatim).
    expect(screen.getByTestId('rule-form-family-help')).toHaveTextContent(
      'Matching entries must carry this module key.',
    );
    // Locked field helpers (plan clarify, verbatim).
    expect(
      screen.getByText('A stable name you will recognize in the list.'),
    ).toBeInTheDocument();
    expect(
      screen.getByText('Check does not read this. The constraint is the fields below.'),
    ).toBeInTheDocument();

    fillNameAndSummary('Belief module required', 'Entries must carry the belief module.');
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'belief' } });
    submit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body).toEqual({
      canonical_name: 'Belief module required',
      statement: 'Entries must carry the belief module.',
      constraint: { family: 'module_presence', module_key: 'belief' },
      status: 'active',
    });
    // Success closes the form.
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
  });

  it('module_absence: locked family copy + carrier', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    renderForm();

    selectFamily('module_absence');
    expect(screen.getByTestId('rule-form-family-help')).toHaveTextContent(
      'Matching entries must not carry this module key.',
    );
    fillNameAndSummary('Tone module forbidden', 'Entries must not carry the tone module.');
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'tone' } });
    submit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body?.constraint).toEqual({ family: 'module_absence', module_key: 'tone' });
  });

  it('required_field entry-level: radio operand + closed field set', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    renderForm();

    selectFamily('required_field');
    expect(screen.getByTestId('rule-form-family-help')).toHaveTextContent(
      'Matching entries must have this field populated.',
    );
    fillNameAndSummary('Summary required', 'Every entry must carry a body summary.');
    fireEvent.click(screen.getByRole('radio', { name: /Entry-level field/ }));
    fireEvent.change(screen.getByLabelText('Field'), { target: { value: 'body.summary' } });
    submit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body?.constraint).toEqual({ family: 'required_field', field: 'body.summary' });
  });

  it('required_field module-row: module_key + free field', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    renderForm();

    selectFamily('required_field');
    fillNameAndSummary('Journal tags required', 'Journal rows must carry tags.');
    fireEvent.click(screen.getByRole('radio', { name: /Module row/ }));
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'journal' } });
    fireEvent.change(screen.getByLabelText('Field'), { target: { value: 'tags' } });
    submit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body?.constraint).toEqual({
      family: 'required_field',
      module_key: 'journal',
      field: 'tags',
    });
  });

  it('observer_cardinality: numeric min/max bounds as numbers', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    renderForm();

    selectFamily('observer_cardinality');
    expect(screen.getByTestId('rule-form-family-help')).toHaveTextContent(
      'Matching events must have an observer count in range.',
    );
    fillNameAndSummary('Observer cap', 'Timeline events must record 0-3 observers.');
    fireEvent.change(screen.getByLabelText('Min observers'), { target: { value: '0' } });
    fireEvent.change(screen.getByLabelText('Max observers'), { target: { value: '3' } });
    submit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body?.constraint).toEqual({ family: 'observer_cardinality', min: 0, max: 3 });
  });

  it('sends draft status, explicit severity, and target entry types when chosen', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    renderForm();

    selectFamily('module_presence');
    fillNameAndSummary('Staged rule', 'Staged constraint.');
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'm' } });
    fireEvent.change(screen.getByLabelText('Status'), { target: { value: 'draft' } });
    fireEvent.change(screen.getByLabelText('Severity'), { target: { value: 'error' } });
    fireEvent.click(screen.getByRole('checkbox', { name: 'Character' }));
    submit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body).toMatchObject({
      status: 'draft',
      severity_hint: 'error',
      target_entry_types: ['character'],
    });
  });

  it('sends a typed kind in the create payload (mirrors the edit builder)', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    renderForm();

    selectFamily('module_presence');
    fillNameAndSummary('Kind carry rule', 'Carries a kind.');
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'm' } });
    fireEvent.change(screen.getByLabelText('Kind'), { target: { value: 'prohibition' } });
    submit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body?.kind).toBe('prohibition');
  });
});

describe('RuleForm — client validation mirror intercepts before submit', () => {
  it('requires a family', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    renderForm();

    fillNameAndSummary('No family', 'No family.');
    submit();

    await waitFor(() =>
      expect(screen.getByText('Choose a constraint family.')).toBeInTheDocument(),
    );
    expect(capture.body).toBeUndefined();
  });

  it('requires non-blank name and summary after trim', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    renderForm();

    selectFamily('module_presence');
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: '   ' } });
    fireEvent.change(screen.getByLabelText('Summary'), { target: { value: '' } });
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'belief' } });
    submit();

    await waitFor(() => expect(screen.getByText('Name is required.')).toBeInTheDocument());
    expect(screen.getByText('Summary is required.')).toBeInTheDocument();
    expect(capture.body).toBeUndefined();
  });

  it('intercepts an empty module_key for the module families', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    renderForm();

    selectFamily('module_absence');
    fillNameAndSummary('Empty key', 'No module key.');
    submit();

    await waitFor(() =>
      expect(screen.getByText('Module key is required.')).toBeInTheDocument(),
    );
    expect(capture.body).toBeUndefined();
  });

  it('intercepts min > max for observer_cardinality', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    renderForm();

    selectFamily('observer_cardinality');
    fillNameAndSummary('Inverted bounds', 'Min exceeds max.');
    fireEvent.change(screen.getByLabelText('Min observers'), { target: { value: '5' } });
    fireEvent.change(screen.getByLabelText('Max observers'), { target: { value: '2' } });
    submit();

    await waitFor(() =>
      expect(screen.getByText('Min must not exceed max.')).toBeInTheDocument(),
    );
    expect(capture.body).toBeUndefined();
  });

  it('requires at least one observer bound', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    renderForm();

    selectFamily('observer_cardinality');
    fillNameAndSummary('No bounds', 'Neither bound.');
    submit();

    await waitFor(() =>
      expect(screen.getByText('At least one bound is required.')).toBeInTheDocument(),
    );
    expect(capture.body).toBeUndefined();
  });

  it('rejects non-whole observer bounds', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    renderForm();

    selectFamily('observer_cardinality');
    fillNameAndSummary('Fractional bound', 'Half an observer.');
    fireEvent.change(screen.getByLabelText('Min observers'), { target: { value: '1.5' } });
    submit();

    await waitFor(() =>
      expect(screen.getByText('Must be a whole number.')).toBeInTheDocument(),
    );
    expect(capture.body).toBeUndefined();
  });

  it('intercepts required_field with no operand form chosen (both absent)', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    renderForm();

    selectFamily('required_field');
    fillNameAndSummary('No operand', 'No operand chosen.');
    submit();

    await waitFor(() =>
      expect(
        screen.getByText('Choose an operand form: entry-level field or module row.'),
      ).toBeInTheDocument(),
    );
    expect(capture.body).toBeUndefined();
  });

  it('intercepts required_field operand misuse: entry-level value combined with module row (both present)', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    renderForm();

    selectFamily('required_field');
    fillNameAndSummary('Mixed operand', 'Entry field on a module row.');
    fireEvent.click(screen.getByRole('radio', { name: /Module row/ }));
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'journal' } });
    fireEvent.change(screen.getByLabelText('Field'), { target: { value: 'body.summary' } });
    submit();

    await waitFor(() =>
      expect(
        screen.getByText('Entry-level field values cannot be combined with a module row.'),
      ).toBeInTheDocument(),
    );
    expect(capture.body).toBeUndefined();
  });
});

describe('RuleForm — API field-level error echo (AR-2 vocabulary, SSOT)', () => {
  it('echoes a constraint.module_key 400 onto the module key field and stays open', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture, () =>
      HttpResponse.json(
        {
          success: false,
          error: {
            code: 'invalid_input',
            message: 'invalid carrier',
            details: {
              field: 'constraint.module_key',
              reason: '"module_key" must be a non-empty string',
            },
          },
        },
        { status: 400 },
      ),
    );
    const onClose = vi.fn();
    renderForm(onClose);

    selectFamily('module_presence');
    fillNameAndSummary('Name', 'Summary');
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'belief' } });
    submit();

    await waitFor(() =>
      expect(screen.getByText('"module_key" must be a non-empty string')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('world-rule-form')).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  it('echoes a canonical_name 400 onto the name field', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture, () =>
      HttpResponse.json(
        {
          success: false,
          error: {
            code: 'invalid_input',
            message: 'bad name',
            details: { field: 'canonical_name', reason: 'canonical_name must not be empty' },
          },
        },
        { status: 400 },
      ),
    );
    renderForm();

    selectFamily('module_presence');
    fillNameAndSummary('Name', 'Summary');
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'belief' } });
    submit();

    await waitFor(() =>
      expect(screen.getByText('canonical_name must not be empty')).toBeInTheDocument(),
    );
  });

  it('echoes a target_entry_types 400 onto the target types group', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture, () =>
      HttpResponse.json(
        {
          success: false,
          error: {
            code: 'invalid_input',
            message: 'bad targets',
            details: {
              field: 'target_entry_types',
              reason:
                'target_entry_types cannot be combined with an observer_cardinality constraint: observer_cardinality applies to timeline events, which carry no entry_type',
            },
          },
        },
        { status: 400 },
      ),
    );
    renderForm();

    selectFamily('observer_cardinality');
    fillNameAndSummary('Observer cap', 'Bounds only.');
    fireEvent.change(screen.getByLabelText('Min observers'), { target: { value: '0' } });
    fireEvent.change(screen.getByLabelText('Max observers'), { target: { value: '3' } });
    submit();

    await waitFor(() =>
      expect(
        screen.getByText(/cannot be combined with an observer_cardinality constraint/),
      ).toBeInTheDocument(),
    );
  });

  it('surfaces a generic inline error for non-envelope failures and keeps the form open', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture, () =>
      HttpResponse.json({ error: { code: 'internal', message: 'boom' } }, { status: 500 }),
    );
    const onClose = vi.fn();
    renderForm(onClose);

    selectFamily('module_presence');
    fillNameAndSummary('Name', 'Summary');
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'belief' } });
    submit();

    await waitFor(() =>
      expect(screen.getByTestId('rule-form-submit-error')).toHaveTextContent(
        'Could not add the rule. Try again.',
      ),
    );
    expect(onClose).not.toHaveBeenCalled();
  });

  it('echoes a status 400 onto the Status field (field-adjacent, AR-2)', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture, () =>
      HttpResponse.json(
        {
          success: false,
          error: {
            code: 'invalid_input',
            message: 'bad status',
            details: {
              field: 'status',
              reason: 'status must be one of draft | active | deprecated',
            },
          },
        },
        { status: 400 },
      ),
    );
    renderForm();

    selectFamily('module_presence');
    fillNameAndSummary('Name', 'Summary');
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'belief' } });
    submit();

    await waitFor(() =>
      expect(screen.getByText('status must be one of draft | active | deprecated')).toHaveAttribute(
        'id',
        'rule-form-status-error',
      ),
    );
  });

  it('echoes a severity_hint 400 onto the Severity field (field-adjacent, AR-2)', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture, () =>
      HttpResponse.json(
        {
          success: false,
          error: {
            code: 'invalid_input',
            message: 'bad severity',
            details: { field: 'severity_hint', reason: 'severity_hint must not be empty' },
          },
        },
        { status: 400 },
      ),
    );
    renderForm();

    selectFamily('module_presence');
    fillNameAndSummary('Name', 'Summary');
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'belief' } });
    submit();

    await waitFor(() =>
      expect(screen.getByText('severity_hint must not be empty')).toHaveAttribute(
        'id',
        'rule-form-severity-error',
      ),
    );
  });

  it('surfaces an unmapped constraint 400 at the submit level (never invisible)', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture, () =>
      HttpResponse.json(
        {
          success: false,
          error: {
            code: 'invalid_input',
            message: 'bad carrier',
            details: { field: 'constraint', reason: 'constraint must be a JSON object' },
          },
        },
        { status: 400 },
      ),
    );
    renderForm();

    selectFamily('module_presence');
    fillNameAndSummary('Name', 'Summary');
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'belief' } });
    submit();

    await waitFor(() =>
      expect(screen.getByTestId('rule-form-submit-error')).toHaveTextContent(
        'constraint must be a JSON object',
      ),
    );
  });

  it('create-mode 404 shows the missing-world copy (AR-6), not the edit rule copy', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture, () =>
      HttpResponse.json(
        {
          success: false,
          error: {
            code: 'not_found',
            message: 'world world-9',
            details: { resource: 'world', reason: 'unknown world' },
          },
        },
        { status: 404 },
      ),
    );
    renderForm();

    selectFamily('module_presence');
    fillNameAndSummary('Name', 'Summary');
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'belief' } });
    submit();

    await waitFor(() =>
      expect(screen.getByTestId('rule-form-submit-error')).toHaveTextContent(
        'This World could not be found. It may have been removed.',
      ),
    );
    expect(
      screen.queryByText('This rule could not be found. It may have been removed.'),
    ).not.toBeInTheDocument();
  });
});

describe('RuleForm — family switch + observer × target exclusivity', () => {
  it('resets operand fields when the family changes', async () => {
    const capture: { body?: WorldRuleCreateRequest } = {};
    useCreateCapture(capture);
    renderForm();

    selectFamily('module_presence');
    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'belief' } });
    selectFamily('observer_cardinality');
    expect(screen.getByLabelText('Min observers')).toBeInTheDocument();
    expect(screen.queryByLabelText('Module key')).not.toBeInTheDocument();
    selectFamily('module_presence');
    const moduleKey = screen.getByLabelText('Module key') as HTMLInputElement;
    expect(moduleKey.value).toBe('');
  });

  it('keeps meta fields across a family switch', async () => {
    renderForm();
    selectFamily('module_presence');
    fillNameAndSummary('Stable name', 'Stable summary.');
    selectFamily('observer_cardinality');
    selectFamily('module_absence');
    expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Stable name');
    expect((screen.getByLabelText('Summary') as HTMLTextAreaElement).value).toBe('Stable summary.');
  });

  it('disables the target entry types axis for observer_cardinality and clears it', async () => {
    renderForm();
    selectFamily('module_presence');
    fireEvent.click(screen.getByRole('checkbox', { name: 'Character' }));
    selectFamily('observer_cardinality');
    const character = screen.getByRole('checkbox', { name: 'Character' }) as HTMLInputElement;
    expect(character).toBeDisabled();
    expect(character.checked).toBe(false);
    expect(
      screen.getByText('Not available for Limit observers — events carry no entry type.'),
    ).toBeInTheDocument();
  });
});

describe('RuleForm — no raw-JSON textarea', () => {
  it('renders exactly one textarea (the human summary) — no constraint JSON editor', async () => {
    renderForm();
    selectFamily('module_presence');
    const textareas = document.querySelectorAll('textarea');
    expect(textareas).toHaveLength(1);
    expect(textareas[0]).toHaveAccessibleName('Summary');
  });
});

describe('validateRuleForm — client mirror (pure)', () => {
  it('rejects observer_cardinality combined with target entry types (exclusivity guard)', () => {
    const state: RuleFormState = {
      ...initialRuleFormState(),
      family: 'observer_cardinality',
      canonicalName: 'Name',
      statement: 'Summary',
      min: '1',
      targetEntryTypes: ['character'],
    };
    expect(validateRuleForm(state)).toEqual({ target_entry_types: 'targetTypesConflict' });
  });

  it('rejects required_field module-row field in the reserved entry-level set', () => {
    const state: RuleFormState = {
      ...initialRuleFormState(),
      family: 'required_field',
      canonicalName: 'Name',
      statement: 'Summary',
      requiredFieldOperand: 'module-row',
      requiredModuleKey: 'journal',
      requiredModuleField: 'body.tags',
    };
    expect(validateRuleForm(state)).toEqual({ 'constraint.field': 'moduleFieldReserved' });
  });

  it('rejects a whole bound beyond the safe-integer mirror range with the honest code', () => {
    const state: RuleFormState = {
      ...initialRuleFormState(),
      family: 'observer_cardinality',
      canonicalName: 'Name',
      statement: 'Summary',
      min: '9007199254740993', // whole u64, but beyond 2^53−1 (qc F-004)
    };
    expect(validateRuleForm(state)).toEqual({ 'constraint.min': 'boundTooLarge' });
  });
});

describe('buildCreateWorldRuleRequest — payload shape', () => {
  it('always sends status; omits severity_hint and target_entry_types when unset', () => {
    const state: RuleFormState = {
      ...initialRuleFormState(),
      family: 'module_presence',
      moduleKey: 'belief',
      canonicalName: 'Name',
      statement: 'Summary',
    };
    const request = buildCreateWorldRuleRequest(state);
    expect(request).toEqual({
      canonical_name: 'Name',
      statement: 'Summary',
      constraint: { family: 'module_presence', module_key: 'belief' },
      status: 'active',
    });
    expect(request?.severity_hint).toBeUndefined();
    expect(request?.target_entry_types).toBeUndefined();
  });

  it('trims canonical_name and statement', () => {
    const state: RuleFormState = {
      ...initialRuleFormState(),
      family: 'module_presence',
      moduleKey: 'm',
      canonicalName: '  Name  ',
      statement: '  Summary  ',
    };
    const request = buildCreateWorldRuleRequest(state);
    expect(request?.canonical_name).toBe('Name');
    expect(request?.statement).toBe('Summary');
  });

  it('sends a trimmed non-empty kind; omits a blank one', () => {
    const withKind: RuleFormState = {
      ...initialRuleFormState(),
      family: 'module_presence',
      moduleKey: 'm',
      canonicalName: 'Name',
      statement: 'Summary',
      kind: '  prohibition  ',
    };
    expect(buildCreateWorldRuleRequest(withKind)?.kind).toBe('prohibition');

    const blankKind: RuleFormState = {
      ...initialRuleFormState(),
      family: 'module_presence',
      moduleKey: 'm',
      canonicalName: 'Name',
      statement: 'Summary',
      kind: '   ',
    };
    const request = buildCreateWorldRuleRequest(blankKind);
    expect(request?.kind).toBeUndefined();
  });
});

describe('RuleForm — edit mode: prefill round-trip (stored carrier → form → PATCH)', () => {
  it('module_presence: prefills meta fields + carrier; untouched submit sends only the stored carrier', async () => {
    const capture: { body?: WorldRuleUpdateRequest } = {};
    useUpdateCapture(capture);
    const onClose = vi.fn();
    renderEditForm(makeStoredRule(), onClose);

    // Prefill: meta fields + the stored carrier deserialized into the form.
    expect((screen.getByLabelText('Constraint family') as HTMLSelectElement).value).toBe(
      'module_presence',
    );
    expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Stored rule');
    expect((screen.getByLabelText('Summary') as HTMLTextAreaElement).value).toBe('Stored statement');
    expect((screen.getByLabelText('Module key') as HTMLInputElement).value).toBe('belief');
    expect((screen.getByLabelText('Status') as HTMLSelectElement).value).toBe('active');
    expect((screen.getByLabelText('Severity') as HTMLSelectElement).value).toBe('warning');
    expect((screen.getByLabelText('Kind') as HTMLInputElement).value).toBe('rule');

    submitEdit();

    await waitFor(() => expect(capture.body).toBeDefined());
    // Untouched meta fields are not sent (AR-3); the carrier is whole replacement.
    expect(capture.body).toEqual({ constraint: { family: 'module_presence', module_key: 'belief' } });
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
  });

  it('module_absence: prefill round-trip', async () => {
    const capture: { body?: WorldRuleUpdateRequest } = {};
    useUpdateCapture(capture);
    renderEditForm(makeStoredRule({ constraint: { family: 'module_absence', module_key: 'tone' } }));

    expect((screen.getByLabelText('Constraint family') as HTMLSelectElement).value).toBe(
      'module_absence',
    );
    expect((screen.getByLabelText('Module key') as HTMLInputElement).value).toBe('tone');
    submitEdit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body).toEqual({ constraint: { family: 'module_absence', module_key: 'tone' } });
  });

  it('required_field entry-level: prefill round-trip', async () => {
    const capture: { body?: WorldRuleUpdateRequest } = {};
    useUpdateCapture(capture);
    renderEditForm(
      makeStoredRule({ constraint: { family: 'required_field', field: 'body.summary' } }),
    );

    expect((screen.getByLabelText('Constraint family') as HTMLSelectElement).value).toBe(
      'required_field',
    );
    expect(screen.getByRole('radio', { name: /Entry-level field/ })).toBeChecked();
    expect((screen.getByLabelText('Field') as HTMLSelectElement).value).toBe('body.summary');
    submitEdit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body).toEqual({ constraint: { family: 'required_field', field: 'body.summary' } });
  });

  it('required_field module-row: prefill round-trip', async () => {
    const capture: { body?: WorldRuleUpdateRequest } = {};
    useUpdateCapture(capture);
    renderEditForm(
      makeStoredRule({
        constraint: { family: 'required_field', module_key: 'journal', field: 'tags' },
      }),
    );

    expect(screen.getByRole('radio', { name: /Module row/ })).toBeChecked();
    expect((screen.getByLabelText('Module key') as HTMLInputElement).value).toBe('journal');
    expect((screen.getByLabelText('Field') as HTMLInputElement).value).toBe('tags');
    submitEdit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body).toEqual({
      constraint: { family: 'required_field', module_key: 'journal', field: 'tags' },
    });
  });

  it('observer_cardinality: prefill round-trip', async () => {
    const capture: { body?: WorldRuleUpdateRequest } = {};
    useUpdateCapture(capture);
    renderEditForm(
      makeStoredRule({ constraint: { family: 'observer_cardinality', min: 0, max: 3 } }),
    );

    expect((screen.getByLabelText('Min observers') as HTMLInputElement).value).toBe('0');
    expect((screen.getByLabelText('Max observers') as HTMLInputElement).value).toBe('3');
    submitEdit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body).toEqual({ constraint: { family: 'observer_cardinality', min: 0, max: 3 } });
  });
});

describe('RuleForm — edit mode: PATCH semantics (AR-3)', () => {
  it('whole-carrier replacement: changing an operand replaces the stored carrier', async () => {
    const capture: { body?: WorldRuleUpdateRequest } = {};
    useUpdateCapture(capture);
    renderEditForm(makeStoredRule());

    fireEvent.change(screen.getByLabelText('Module key'), { target: { value: 'new-module' } });
    submitEdit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body?.constraint).toEqual({ family: 'module_presence', module_key: 'new-module' });
  });

  it('meta-only PATCH: renaming canonical_name sends only the name + carrier', async () => {
    const capture: { body?: WorldRuleUpdateRequest } = {};
    useUpdateCapture(capture);
    renderEditForm(makeStoredRule());

    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Renamed rule' } });
    submitEdit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body).toEqual({
      canonical_name: 'Renamed rule',
      constraint: { family: 'module_presence', module_key: 'belief' },
    });
  });

  it('status switching: draft → active sends status', async () => {
    const capture: { body?: WorldRuleUpdateRequest } = {};
    useUpdateCapture(capture);
    renderEditForm(makeStoredRule({ status: 'draft' }));

    expect((screen.getByLabelText('Status') as HTMLSelectElement).value).toBe('draft');
    fireEvent.change(screen.getByLabelText('Status'), { target: { value: 'active' } });
    submitEdit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body?.status).toBe('active');
  });

  it('status switching: active → draft sends status', async () => {
    const capture: { body?: WorldRuleUpdateRequest } = {};
    useUpdateCapture(capture);
    renderEditForm(makeStoredRule());

    fireEvent.change(screen.getByLabelText('Status'), { target: { value: 'draft' } });
    submitEdit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body?.status).toBe('draft');
  });

  it('severity change sends severity_hint', async () => {
    const capture: { body?: WorldRuleUpdateRequest } = {};
    useUpdateCapture(capture);
    renderEditForm(makeStoredRule());

    fireEvent.change(screen.getByLabelText('Severity'), { target: { value: 'error' } });
    submitEdit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body?.severity_hint).toBe('error');
  });

  it('clearing all target entry types sends [] (explicit clear, AR-3)', async () => {
    const capture: { body?: WorldRuleUpdateRequest } = {};
    useUpdateCapture(capture);
    renderEditForm(makeStoredRule({ target_entry_types: ['character'] }));

    expect(screen.getByRole('checkbox', { name: 'Character' })).toBeChecked();
    fireEvent.click(screen.getByRole('checkbox', { name: 'Character' }));
    submitEdit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body?.target_entry_types).toEqual([]);
  });

  it('kind change sends kind; the edit form prefills the stored kind', async () => {
    const capture: { body?: WorldRuleUpdateRequest } = {};
    useUpdateCapture(capture);
    renderEditForm(makeStoredRule({ kind: 'prohibition' }));

    expect((screen.getByLabelText('Kind') as HTMLInputElement).value).toBe('prohibition');
    fireEvent.change(screen.getByLabelText('Kind'), { target: { value: 'style' } });
    submitEdit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body?.kind).toBe('style');
  });

  it('edit form represents deprecated status and reactivates via active', async () => {
    const capture: { body?: WorldRuleUpdateRequest } = {};
    useUpdateCapture(capture);
    renderEditForm(makeStoredRule({ status: 'deprecated' }));

    expect((screen.getByLabelText('Status') as HTMLSelectElement).value).toBe('deprecated');
    fireEvent.change(screen.getByLabelText('Status'), { target: { value: 'active' } });
    submitEdit();

    await waitFor(() => expect(capture.body).toBeDefined());
    expect(capture.body?.status).toBe('active');
  });
});

describe('RuleForm — edit mode: severity honesty (stored hint, no inert Default)', () => {
  it('hides the inert Default option and states the stored hint when one is stored', async () => {
    renderEditForm(makeStoredRule()); // makeStoredRule stores severity_hint: 'warning'

    const severity = screen.getByLabelText('Severity') as HTMLSelectElement;
    expect(severity.value).toBe('warning');
    // No silent no-op: "Default (warning)" cannot be chosen for a stored hint
    // (no null-clearing, AR-3) — the option is not rendered.
    expect(screen.queryByRole('option', { name: 'Default (warning)' })).not.toBeInTheDocument();
    expect(
      screen.getByText('Stored severity: warning. Pick another value to change it.'),
    ).toBeInTheDocument();
  });

  it('keeps the Default option and the not-set helper when no hint is stored', async () => {
    renderEditForm(makeStoredRule({ severity_hint: null }));

    const severity = screen.getByLabelText('Severity') as HTMLSelectElement;
    expect(severity.value).toBe('');
    expect(screen.getByRole('option', { name: 'Default (warning)' })).toBeInTheDocument();
    expect(
      screen.getByText('Not set — Check evaluates the rule as warning.'),
    ).toBeInTheDocument();
  });
});

describe('RuleForm — edit mode: unknown-id 404 echo (AR-6, no leak)', () => {
  it('echoes an honest non-field error and keeps the form open', async () => {
    const capture: { body?: WorldRuleUpdateRequest } = {};
    useUpdateCapture(capture, () =>
      HttpResponse.json(
        {
          success: false,
          error: {
            code: 'not_found',
            message: 'rule rul_edit',
            details: { resource: 'rule', reason: 'unknown rule' },
          },
        },
        { status: 404 },
      ),
    );
    const onClose = vi.fn();
    renderEditForm(makeStoredRule(), onClose);

    submitEdit();

    await waitFor(() =>
      expect(screen.getByTestId('rule-form-submit-error')).toHaveTextContent(
        'This rule could not be found. It may have been removed.',
      ),
    );
    expect(screen.getByTestId('world-rule-form')).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });
});

describe('buildUpdateWorldRuleRequest — PATCH semantics (pure, AR-3)', () => {
  const stored: WorldRule = {
    rule_id: 'rul_edit',
    canonical_name: 'Stored rule',
    kind: 'rule',
    target_entry_types: ['character'],
    status: 'active',
    severity_hint: 'warning',
    statement: 'Stored statement',
    constraint: { family: 'module_presence', module_key: 'belief' },
  };

  it('untouched form sends only the carrier (whole replacement)', () => {
    const request = buildUpdateWorldRuleRequest(ruleFormStateFromRule(stored), stored);
    expect(request).toEqual({ constraint: { family: 'module_presence', module_key: 'belief' } });
  });

  it('never sends an empty severity_hint (no null-clearing, AR-3)', () => {
    const state = ruleFormStateFromRule(stored);
    state.severityHint = ''; // author picks "Default (warning)" on a stored hint
    const request = buildUpdateWorldRuleRequest(state, stored);
    expect(request?.severity_hint).toBeUndefined();
  });

  it('never sends an empty kind (cannot be unset, AR-3)', () => {
    const state = ruleFormStateFromRule(stored);
    state.kind = '   ';
    const request = buildUpdateWorldRuleRequest(state, stored);
    expect(request?.kind).toBeUndefined();
  });
});

describe('ruleFormStateFromRule — carrier deserialization (pure)', () => {
  it('unknown family leaves the picker unselected (explicit choice before replacement)', () => {
    const state = ruleFormStateFromRule({
      rule_id: 'rul_x',
      canonical_name: 'Odd rule',
      kind: 'rule',
      target_entry_types: [],
      constraint: { family: 'custom_family', mode: 'strict' },
    });
    expect(state.family).toBeNull();
  });

  it('absent constraint leaves the picker unselected', () => {
    const state = ruleFormStateFromRule({
      rule_id: 'rul_x',
      canonical_name: 'No carrier',
      kind: 'rule',
      target_entry_types: [],
    });
    expect(state.family).toBeNull();
  });
});
