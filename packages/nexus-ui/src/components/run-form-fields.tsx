import type { ChangeEvent } from 'react';

import { cn } from '../lib/cn';

import { EntityPickerField, type EntityPickerEntry } from './entity-picker-field';
import { Input } from './input';
import { Label } from './label';
import { Select } from './select';

/**
 * JSON Schema fragment for one invocation property (manifest
 * `schemas.invocation.properties.*`). Structural — compatible with the
 * generated ModuleDetail manifest shape without importing wire DTOs.
 */
export interface InvocationSchemaProperty {
  type?: string | string[];
  enum?: unknown[];
  title?: string;
  description?: string;
  minimum?: number;
  maximum?: number;
  default?: unknown;
  /** Other JSON Schema keywords (items, format, …) pass through untyped. */
  [keyword: string]: unknown;
}

/**
 * JSON Schema fragment for a module invocation form (manifest
 * `schemas.invocation`).
 */
export interface InvocationSchema {
  type?: string;
  properties?: Record<string, InvocationSchemaProperty>;
  required?: string[];
}

/** Caller-owned copy for the schema-driven form (i18n lives in the app). */
export interface RunFormCopy {
  /** Headline when the manifest has no usable invocation schema. */
  emptyTitle: string;
  /** Body when the manifest has no usable invocation schema. */
  emptyDescription: string;
  /** Note for a property kind the guided form cannot render (Advanced JSON path). */
  unsupportedFieldNote: string;
  /** Placeholder for entity pickers. */
  entityPlaceholder: string;
  /** Placeholder for enum selects. */
  selectPlaceholder: string;
  /** Entity picker empty-state copy (e.g. no characters in the World). */
  entityEmptyTitle: string;
  entityEmptyDescription: string;
}

export interface RunFormFieldsProps {
  /** Manifest invocation schema fragment; missing/empty renders the empty state. */
  schema: InvocationSchema | null | undefined;
  /** Module `required_key_block_types` — gates `*_id` → entity picker derivation. */
  requiredKeyBlockTypes?: string[];
  /** Current field values keyed by property name. */
  values: Record<string, unknown>;
  onChange: (name: string, value: unknown) => void;
  /** Entity entries per picker field name (fixture/app supplies; picker stays pure). */
  entityEntries?: Record<string, EntityPickerEntry[]>;
  copy: RunFormCopy;
  /** Id prefix for label/control association — caller-owned. */
  idPrefix?: string;
  disabled?: boolean;
  className?: string;
}

type FieldKind = 'entity' | 'enum' | 'boolean' | 'number' | 'string' | 'unsupported';

function includesType(type: string | string[] | undefined, wanted: string[]): boolean {
  if (!type) return false;
  const list = Array.isArray(type) ? type : [type];
  return list.some((t) => wanted.includes(t));
}

/**
 * Field derivation per the V1.147 form contract (behavior spec §3):
 * `*_id` + required_key_block_types → entity picker; enum → select;
 * boolean → checkbox; number/integer → number input; string → text input;
 * anything else → unsupported note (Advanced raw-JSON path stays in the app).
 */
function deriveFieldKind(
  name: string,
  property: InvocationSchemaProperty,
  requiredKeyBlockTypes: string[],
): FieldKind {
  if (name.endsWith('_id') && requiredKeyBlockTypes.length > 0) return 'entity';
  if (property.enum && property.enum.length > 0) return 'enum';
  if (includesType(property.type, ['boolean'])) return 'boolean';
  if (includesType(property.type, ['number', 'integer'])) return 'number';
  if (includesType(property.type, ['string']) || property.type === undefined) return 'string';
  return 'unsupported';
}

function humanize(name: string): string {
  const spaced = name.replace(/_id$/, '').replace(/_/g, ' ').trim();
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

function fieldLabel(name: string, property: InvocationSchemaProperty): string {
  return property.title ?? humanize(name);
}

/**
 * RunFormFields — schema-driven guided form for a compute module Run
 * (V1.147 P1). Pure presentational: derives first-class controls from the
 * manifest invocation schema fragment; all copy, values, entity entries,
 * and change handling are caller-owned.
 */
export function RunFormFields({
  schema,
  requiredKeyBlockTypes = [],
  values,
  onChange,
  entityEntries = {},
  copy,
  idPrefix = 'run-form',
  disabled = false,
  className,
}: RunFormFieldsProps) {
  const properties = schema?.properties ?? {};
  const names = Object.keys(properties);
  const required = new Set(schema?.required ?? []);

  if (names.length === 0) {
    return (
      <div
        data-testid="run-form-empty"
        className={cn(
          'rounded-card border border-dashed border-gray-alpha-400 bg-background-200 p-6',
          className,
        )}
      >
        <p className="text-label-14 font-medium text-gray-1000">{copy.emptyTitle}</p>
        <p className="mt-1 text-copy-13 text-gray-700">{copy.emptyDescription}</p>
      </div>
    );
  }

  return (
    <div className={cn('flex flex-col gap-4', className)} data-testid="run-form-fields">
      {names.map((name) => {
        const property = properties[name];
        const kind = deriveFieldKind(name, property, requiredKeyBlockTypes);
        const id = `${idPrefix}-${name}`;
        const helperId = `${id}-helper`;
        const label = fieldLabel(name, property);
        const isRequired = required.has(name);
        const requiredMark = isRequired ? <span className="text-red-700">*</span> : null;
        const helper = property.description ? (
          <p id={helperId} className="text-copy-13 text-gray-700">
            {property.description}
          </p>
        ) : null;

        if (kind === 'entity') {
          return (
            <div key={name} data-field={name}>
              <EntityPickerField
                id={id}
                label={label}
                entries={entityEntries[name] ?? []}
                value={typeof values[name] === 'string' ? (values[name] as string) : null}
                onChange={(entryId) => onChange(name, entryId)}
                placeholder={copy.entityPlaceholder}
                required={isRequired}
                disabled={disabled}
                helperText={property.description}
                emptyTitle={copy.entityEmptyTitle}
                emptyDescription={copy.entityEmptyDescription}
              />
            </div>
          );
        }

        return (
          <div key={name} className="flex flex-col gap-1.5" data-field={name}>
            {kind === 'boolean' ? (
              // Single label owning both the text and the checkbox (a11y —
              // two labels targeting one control would double the announcement).
              <label className="flex h-10 items-center gap-2" htmlFor={id}>
                {label} {requiredMark}
                <input
                  id={id}
                  type="checkbox"
                  checked={values[name] === true}
                  onChange={(event: ChangeEvent<HTMLInputElement>) =>
                    onChange(name, event.target.checked)
                  }
                  disabled={disabled}
                  aria-describedby={property.description ? helperId : undefined}
                  className="h-4 w-4 accent-blue-700"
                  data-testid={`run-form-field-${name}`}
                />
              </label>
            ) : (
              <Label htmlFor={id}>
                {label} {requiredMark}
              </Label>
            )}
            {kind === 'enum' && (
              <Select
                id={id}
                value={typeof values[name] === 'string' ? (values[name] as string) : ''}
                onChange={(event) => onChange(name, event.target.value)}
                disabled={disabled}
                aria-describedby={property.description ? helperId : undefined}
                data-testid={`run-form-field-${name}`}
              >
                <option value="" disabled>
                  {copy.selectPlaceholder}
                </option>
                {(property.enum ?? []).map((option) => (
                  <option key={String(option)} value={String(option)}>
                    {String(option)}
                  </option>
                ))}
              </Select>
            )}
            {kind === 'number' && (
              <Input
                id={id}
                type="number"
                value={typeof values[name] === 'number' ? String(values[name]) : ''}
                onChange={(event) => {
                  const next = event.target.valueAsNumber;
                  onChange(name, Number.isNaN(next) ? undefined : next);
                }}
                min={property.minimum}
                max={property.maximum}
                disabled={disabled}
                aria-describedby={property.description ? helperId : undefined}
                data-testid={`run-form-field-${name}`}
              />
            )}
            {kind === 'string' && (
              <Input
                id={id}
                value={typeof values[name] === 'string' ? (values[name] as string) : ''}
                onChange={(event) => onChange(name, event.target.value)}
                disabled={disabled}
                aria-describedby={property.description ? helperId : undefined}
                data-testid={`run-form-field-${name}`}
              />
            )}
            {kind === 'unsupported' && (
              <p
                className="text-copy-13 text-gray-700"
                data-testid={`run-form-field-${name}-unsupported`}
              >
                {copy.unsupportedFieldNote}
              </p>
            )}
            {helper}
          </div>
        );
      })}
    </div>
  );
}
