/**
 * RuleForm — inline create/edit form for the world-rules section (V1.169 P2
 * T1/T2, DF-82).
 *
 * Four-family constraint authoring without JSON: family picker first (locked
 * labels/help), per-family operand fields appear after the pick and reset on
 * family change, plus `canonical_name` / `statement` / status / severity /
 * kind / target entry types. The client validation mirror intercepts obvious
 * mistakes at submit time (API = SSOT, AR-2); 400 `invalid_input` errors are
 * echoed per field using the AR-2 field vocabulary verbatim; a 404 echoes an
 * honest non-field error (AR-6, no leak).
 *
 * Edit mode (T2): pass the stored read-item projection as `rule` — the form
 * prefills from it (carrier deserialized into the family form) and submits a
 * per-field PATCH (AR-3): the carrier is whole replacement, untouched meta
 * fields are not sent, `severity_hint`/`kind` are never sent empty (no
 * null-clearing). The status select includes `deprecated` so a Deactivated
 * rule can be reactivated via Edit → `active`.
 *
 * No modal, no Dialog, no raw-JSON textarea (plan locks) — the form renders
 * inline in the world-rules Card.
 */
import { Children, cloneElement, isValidElement, useState, type FormEvent, type ReactElement, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { isRuleInvalidInputError, isRuleNotFoundError, useCreateWorldRule, useUpdateWorldRule } from '@/api/queries';
import { BLOCK_TYPE_LABELS } from '@/components/canvas/world-kb/types';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import type { BlockType } from '@42ch/nexus-contracts';

import {
  EDIT_RULE_STATUS_OPTIONS,
  ENTRY_FIELDS,
  RULE_FAMILIES,
  RULE_STATUS_OPTIONS,
  SEVERITY_OPTIONS,
  buildCreateWorldRuleRequest,
  buildUpdateWorldRuleRequest,
  initialRuleFormState,
  ruleFormStateFromRule,
  validateRuleForm,
  withFamily,
  type RequiredFieldOperand,
  type RuleFamily,
  type RuleFormErrorKey,
  type RuleFormErrors,
  type RuleFormState,
  type WorldRule,
} from './rule-form-state';

/** Target-entry-types axis: the full spoke BlockType set with labels. */
const ENTRY_TYPE_OPTIONS: { value: BlockType; label: string }[] = (
  Object.keys(BLOCK_TYPE_LABELS) as BlockType[]
).map((value) => ({ value, label: BLOCK_TYPE_LABELS[value] }));

interface RuleFormProps {
  worldId: string;
  /**
   * The stored read-item projection being edited. Absent = create mode.
   * The section keys the form by `rule_id` so switching rules remounts.
   */
  rule?: WorldRule;
  /** Called after a successful create/edit (the section closes the form). */
  onClose: () => void;
}

export function RuleForm({ worldId, rule, onClose }: RuleFormProps) {
  const { t } = useTranslation('worldRules');
  const createRule = useCreateWorldRule(worldId);
  const updateRule = useUpdateWorldRule(worldId);
  const [state, setState] = useState<RuleFormState>(() =>
    rule ? ruleFormStateFromRule(rule) : initialRuleFormState(),
  );
  const [errors, setErrors] = useState<RuleFormErrors>({});
  const [submitError, setSubmitError] = useState<string | null>(null);
  const isEdit = rule !== undefined;
  const isPending = isEdit ? updateRule.isPending : createRule.isPending;

  function update(patch: Partial<RuleFormState>) {
    setState((s) => ({ ...s, ...patch }));
  }

  function clearFieldError(...keys: RuleFormErrorKey[]) {
    setErrors((prev) => {
      const next = { ...prev };
      for (const key of keys) delete next[key];
      return next;
    });
  }

  function handleFamilyChange(value: string) {
    if (value === '') {
      setState((s) => withFamily(s, null));
    } else {
      setState((s) => withFamily(s, value as RuleFamily));
    }
    setErrors({});
  }

  function handleError(error: unknown) {
    if (isRuleInvalidInputError(error)) {
      const details = error.details as { field?: string; reason?: string } | undefined;
      if (details && typeof details.field === 'string' && typeof details.reason === 'string') {
        // AR-2 vocabulary maps 1:1 onto form fields — echo verbatim. The
        // form-level keys (`constraint`, `patch`) have no field surface:
        // surface them at the submit level so the echo is never invisible.
        if (details.field === 'constraint' || details.field === 'patch') {
          setSubmitError(details.reason);
        } else {
          setErrors((prev) => ({ ...prev, [details.field as RuleFormErrorKey]: details.reason }));
        }
      } else {
        setSubmitError(error.message);
      }
    } else if (isRuleNotFoundError(error)) {
      // P1 404 envelope (AR-6): a create-mode 404 is the missing-world guard
      // (world gone mid-form); an edit-mode 404 is an unknown/foreign rule —
      // honest copy per mode, no id/world leak.
      setSubmitError(isEdit ? t('form.notFoundError') : t('form.worldNotFoundError'));
    } else {
      // The hook toasts non-envelope failures; mirror a generic inline
      // message so the form never closes silently on failure.
      setSubmitError(isEdit ? t('form.genericUpdateError') : t('form.genericError'));
    }
  }

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    setSubmitError(null);

    const validation = validateRuleForm(state);
    if (Object.keys(validation).length > 0) {
      const rendered: RuleFormErrors = {};
      for (const [key, code] of Object.entries(validation) as [
        RuleFormErrorKey,
        (typeof validation)[RuleFormErrorKey],
      ][]) {
        rendered[key] = t(`form.errors.${code}`);
      }
      setErrors(rendered);
      return;
    }

    // Clean slate before the API round-trip: from here on the error state
    // holds only what the server echoes, so a stale field message from an
    // earlier submit can never survive next to a field the new response did
    // not reject (bugbot 677244f5).
    setErrors({});

    if (isEdit) {
      const request = buildUpdateWorldRuleRequest(state, rule);
      if (request === null) return; // unreachable: validation requires a family
      updateRule.mutate(
        { ruleId: rule.rule_id, request },
        {
          onSuccess: () => onClose(),
          onError: handleError,
        },
      );
      return;
    }

    const request = buildCreateWorldRuleRequest(state);
    if (request === null) return; // unreachable: validation requires a family

    createRule.mutate(request, {
      onSuccess: () => onClose(),
      onError: handleError,
    });
  }

  const familyHelp = state.family ? t(`form.families.${state.family}.help`) : null;
  const targetAxisDisabled = state.family === 'observer_cardinality';
  // Edit-mode severity honesty: when a hint is stored, the "Default
  // (warning)" option is a silent no-op (no null-clearing, AR-3) and its
  // label misstates the rule's evaluation — hide it and state the stored
  // value instead (qc F-001).
  const storedSeverity = isEdit && rule?.severity_hint ? rule.severity_hint : null;

  return (
    <form
      className="flex flex-col gap-4 rounded-card border border-gray-alpha-400 bg-background-200 p-4"
      onSubmit={handleSubmit}
      noValidate
      data-testid="world-rule-form"
    >
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">
          {isEdit ? t('form.editTitle') : t('form.title')}
        </h3>
        <Button type="button" variant="tertiary" size="small" onClick={onClose} disabled={isPending}>
          {t('form.cancel')}
        </Button>
      </div>

      <RuleFormField label={t('form.familyLabel')} htmlFor="rule-form-family" error={errors['constraint.family']}>
        <Select
          id="rule-form-family"
          value={state.family ?? ''}
          onChange={(e) => handleFamilyChange(e.target.value)}
        >
          <option value="">{t('form.familyPlaceholder')}</option>
          {RULE_FAMILIES.map((family) => (
            <option key={family} value={family}>
              {t(`form.families.${family}.label`)}
            </option>
          ))}
        </Select>
      </RuleFormField>
      {familyHelp ? (
        <p className="-mt-2 text-copy-13 text-gray-700" data-testid="rule-form-family-help">
          {familyHelp}
        </p>
      ) : null}

      <RuleFormField
        label={t('form.nameLabel')}
        htmlFor="rule-form-name"
        helper={t('form.nameHelp')}
        error={errors.canonical_name}
      >
        <Input
          id="rule-form-name"
          value={state.canonicalName}
          onChange={(e) => {
            update({ canonicalName: e.target.value });
            clearFieldError('canonical_name');
          }}
        />
      </RuleFormField>

      <RuleFormField
        label={t('form.statementLabel')}
        htmlFor="rule-form-statement"
        helper={t('form.statementHelp')}
        error={errors.statement}
      >
        <Textarea
          id="rule-form-statement"
          value={state.statement}
          onChange={(e) => {
            update({ statement: e.target.value });
            clearFieldError('statement');
          }}
        />
      </RuleFormField>

      {state.family === 'module_presence' || state.family === 'module_absence' ? (
        <RuleFormField
          label={t('form.moduleKeyLabel')}
          htmlFor="rule-form-module-key"
          error={errors['constraint.module_key']}
        >
          <Input
            id="rule-form-module-key"
            value={state.moduleKey}
            onChange={(e) => {
              update({ moduleKey: e.target.value });
              clearFieldError('constraint.module_key');
            }}
          />
        </RuleFormField>
      ) : null}

      {state.family === 'required_field' ? (
        <RequiredFieldOperands
          operand={state.requiredFieldOperand}
          entryField={state.entryField}
          moduleKey={state.requiredModuleKey}
          moduleField={state.requiredModuleField}
          error={errors['constraint.field']}
          moduleKeyError={errors['constraint.module_key']}
          onOperandChange={(operand) => {
            update({ requiredFieldOperand: operand });
            clearFieldError('constraint.field', 'constraint.module_key');
          }}
          onEntryFieldChange={(value) => {
            update({ entryField: value });
            clearFieldError('constraint.field');
          }}
          onModuleKeyChange={(value) => {
            update({ requiredModuleKey: value });
            clearFieldError('constraint.module_key');
          }}
          onModuleFieldChange={(value) => {
            update({ requiredModuleField: value });
            clearFieldError('constraint.field');
          }}
        />
      ) : null}

      {state.family === 'observer_cardinality' ? (
        <div className="grid grid-cols-2 gap-3">
          <RuleFormField label={t('form.minLabel')} htmlFor="rule-form-min" error={errors['constraint.min']}>
            <Input
              id="rule-form-min"
              type="number"
              min={0}
              step={1}
              value={state.min}
              onChange={(e) => {
                update({ min: e.target.value });
                clearFieldError('constraint.min');
              }}
            />
          </RuleFormField>
          <RuleFormField label={t('form.maxLabel')} htmlFor="rule-form-max" error={errors['constraint.max']}>
            <Input
              id="rule-form-max"
              type="number"
              min={0}
              step={1}
              value={state.max}
              onChange={(e) => {
                update({ max: e.target.value });
                clearFieldError('constraint.max');
              }}
            />
          </RuleFormField>
        </div>
      ) : null}

      <RuleFormField
        label={t('form.statusLabel')}
        htmlFor="rule-form-status"
        helper={t('form.statusHelp')}
        error={errors.status}
      >
        <Select
          id="rule-form-status"
          value={state.status}
          onChange={(e) => update({ status: e.target.value as RuleFormState['status'] })}
        >
          {(isEdit ? EDIT_RULE_STATUS_OPTIONS : RULE_STATUS_OPTIONS).map((status) => (
            <option key={status} value={status}>
              {t(`form.status${status.charAt(0).toUpperCase()}${status.slice(1)}`)}
            </option>
          ))}
        </Select>
      </RuleFormField>

      <RuleFormField
        label={t('form.severityLabel')}
        htmlFor="rule-form-severity"
        helper={
          storedSeverity
            ? t('form.severityStoredHelp', { severity: storedSeverity })
            : t('form.severityHelp')
        }
        error={errors.severity_hint}
      >
        <Select
          id="rule-form-severity"
          value={state.severityHint}
          onChange={(e) =>
            update({ severityHint: e.target.value as RuleFormState['severityHint'] })
          }
        >
          {storedSeverity === null ? (
            <option value="">{t('form.severityDefault')}</option>
          ) : null}
          {SEVERITY_OPTIONS.map((severity) => (
            <option key={severity} value={severity}>
              {severity}
            </option>
          ))}
        </Select>
      </RuleFormField>

      <RuleFormField label={t('form.kindLabel')} htmlFor="rule-form-kind" helper={t('form.kindHelp')} error={errors.kind}>
        <Input
          id="rule-form-kind"
          value={state.kind}
          onChange={(e) => {
            update({ kind: e.target.value });
            clearFieldError('kind');
          }}
        />
      </RuleFormField>

      <fieldset className="flex flex-col gap-2" disabled={targetAxisDisabled} data-testid="rule-form-target-types">
        <legend className="text-copy-13 text-gray-700">{t('form.targetTypesLabel')}</legend>
        <p className="text-copy-13 text-gray-700">
          {targetAxisDisabled ? t('form.targetTypesDisabledHelp') : t('form.targetTypesHelp')}
        </p>
        <div className="flex flex-wrap gap-x-4 gap-y-1">
          {ENTRY_TYPE_OPTIONS.map(({ value, label }) => (
            <label key={value} className="flex items-center gap-2 text-copy-14 text-gray-900">
              <input
                type="checkbox"
                className="h-4 w-4 rounded border-gray-alpha-400"
                value={value}
                checked={state.targetEntryTypes.includes(value)}
                onChange={(e) => {
                  update({
                    targetEntryTypes: e.target.checked
                      ? [...state.targetEntryTypes, value]
                      : state.targetEntryTypes.filter((v) => v !== value),
                  });
                  clearFieldError('target_entry_types');
                }}
              />
              {label}
            </label>
          ))}
        </div>
        {errors.target_entry_types ? (
          <p id="rule-form-target-types-error" role="alert" className="text-copy-13 text-red-1000">
            {errors.target_entry_types}
          </p>
        ) : null}
      </fieldset>

      {submitError ? (
        <p
          role="alert"
          className="rounded-card border border-red-700/30 bg-red-700/10 p-3 text-copy-13 text-red-1000"
          data-testid="rule-form-submit-error"
        >
          {submitError}
        </p>
      ) : null}

      <div className="flex justify-end gap-2">
        <Button type="button" variant="tertiary" size="small" onClick={onClose} disabled={isPending}>
          {t('form.cancel')}
        </Button>
        <Button type="submit" variant="primary" size="small" disabled={isPending} data-testid="rule-form-submit">
          {isPending
            ? isEdit
              ? t('form.savePending')
              : t('form.submitPending')
            : isEdit
              ? t('form.save')
              : t('form.submit')}
        </Button>
      </div>
    </form>
  );
}

/**
 * `required_field` operand fields: the radio choice (entry-level closed
 * field vs module-row `module_key` + free field) plus the operand-specific
 * inputs. Errors key to the AR-2 `constraint.field` / `constraint.module_key`
 * vocabulary; the operand-level error renders only when no form is chosen.
 */
function RequiredFieldOperands({
  operand,
  entryField,
  moduleKey,
  moduleField,
  error,
  moduleKeyError,
  onOperandChange,
  onEntryFieldChange,
  onModuleKeyChange,
  onModuleFieldChange,
}: {
  operand: RequiredFieldOperand | null;
  entryField: string;
  moduleKey: string;
  moduleField: string;
  error?: string;
  moduleKeyError?: string;
  onOperandChange: (operand: RequiredFieldOperand) => void;
  onEntryFieldChange: (value: string) => void;
  onModuleKeyChange: (value: string) => void;
  onModuleFieldChange: (value: string) => void;
}) {
  const { t } = useTranslation('worldRules');
  return (
    <>
      <fieldset className="flex flex-col gap-2" data-testid="rule-form-operand">
        <legend className="text-copy-13 text-gray-700">{t('form.operandLabel')}</legend>
        {(['entry', 'module-row'] as const).map((option) => (
          <label key={option} className="flex items-center gap-2 text-copy-14 text-gray-900">
            <input
              type="radio"
              name="rule-form-operand"
              className="h-4 w-4"
              value={option}
              checked={operand === option}
              onChange={() => onOperandChange(option)}
            />
            <span>
              {t(`form.operand.${option}.label`)}
              <span className="text-copy-13 text-gray-700"> — {t(`form.operand.${option}.help`)}</span>
            </span>
          </label>
        ))}
        {operand === null && error ? (
          <p id="rule-form-operand-error" role="alert" className="text-copy-13 text-red-1000">
            {error}
          </p>
        ) : null}
      </fieldset>
      {operand === 'entry' ? (
        <RuleFormField label={t('form.entryFieldLabel')} htmlFor="rule-form-entry-field" error={error}>
          <Select
            id="rule-form-entry-field"
            value={entryField}
            onChange={(e) => onEntryFieldChange(e.target.value)}
          >
            <option value="">{t('form.entryFieldPlaceholder')}</option>
            {ENTRY_FIELDS.map((field) => (
              <option key={field} value={field}>
                {field}
              </option>
            ))}
          </Select>
        </RuleFormField>
      ) : operand === 'module-row' ? (
        <>
          <RuleFormField
            label={t('form.moduleKeyLabel')}
            htmlFor="rule-form-required-module-key"
            error={moduleKeyError}
          >
            <Input
              id="rule-form-required-module-key"
              value={moduleKey}
              onChange={(e) => onModuleKeyChange(e.target.value)}
            />
          </RuleFormField>
          <RuleFormField
            label={t('form.entryFieldLabel')}
            htmlFor="rule-form-required-module-field"
            error={error}
          >
            <Input
              id="rule-form-required-module-field"
              value={moduleField}
              onChange={(e) => onModuleFieldChange(e.target.value)}
            />
          </RuleFormField>
        </>
      ) : null}
    </>
  );
}

/**
 * Field wrapper: label + control + one-line helper / field-adjacent error.
 * The control receives `aria-describedby` (helper or error id) and the
 * visual `invalid` flag so errors are readable (a11y baseline).
 */
function RuleFormField({
  label,
  htmlFor,
  helper,
  error,
  children,
}: {
  label: string;
  htmlFor: string;
  helper?: string;
  error?: string;
  children: ReactNode;
}) {
  const helperId = `${htmlFor}-helper`;
  const errorId = `${htmlFor}-error`;
  const describedBy = error ? errorId : helper ? helperId : undefined;
  const control = Children.only(children);
  const controlWithAria = isValidElement(control)
    ? cloneElement(control as ReactElement<{ 'aria-describedby'?: string; invalid?: boolean }>, {
        'aria-describedby': describedBy,
        invalid: Boolean(error),
      })
    : control;

  return (
    <div className="flex flex-col gap-1">
      <Label htmlFor={htmlFor} className="text-copy-13 text-gray-700">
        {label}
      </Label>
      {controlWithAria}
      {helper && !error ? (
        <p id={helperId} className="text-copy-13 text-gray-700">
          {helper}
        </p>
      ) : null}
      {error ? (
        <p id={errorId} role="alert" className="text-copy-13 text-red-1000">
          {error}
        </p>
      ) : null}
    </div>
  );
}
