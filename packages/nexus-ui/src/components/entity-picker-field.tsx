import { cn } from '../lib/cn';

import { Label } from './label';
import { Select } from './select';

/**
 * One selectable entity in an EntityPickerField (e.g. a character
 * KnowledgeEntry in the selected World). Caller maps domain entries into
 * this shape; the picker never queries data itself.
 */
export interface EntityPickerEntry {
  id: string;
  title: string;
  /** Optional secondary line (e.g. type or stats hint). */
  subtitle?: string;
}

export interface EntityPickerFieldProps {
  /** Control id — caller-owned (label/control association). */
  id?: string;
  /** Field label copy — caller-owned. */
  label: string;
  /** Entries offered for selection. Empty renders the empty-state block. */
  entries: EntityPickerEntry[];
  /** Currently selected entry id, or null for none. */
  value: string | null;
  onChange: (id: string) => void;
  /** Placeholder option copy shown when value is null — caller-owned. */
  placeholder?: string;
  required?: boolean;
  disabled?: boolean;
  invalid?: boolean;
  /** Helper copy under the control — caller-owned; wired via aria-describedby. */
  helperText?: string;
  /** Empty-state headline when entries is empty — caller-owned. */
  emptyTitle?: string;
  /** Empty-state body when entries is empty — caller-owned. */
  emptyDescription?: string;
  className?: string;
}

/**
 * EntityPickerField — labeled single-entity picker used by the Compute Run
 * Studio form for `*_id` manifest fields (e.g. basic-combat
 * attacker/defender pickers over World character entries).
 *
 * Pure presentational: entries and all copy are caller-owned; no query,
 * daemon, or routing dependencies. Renders the promoted native Select for
 * the control; when `entries` is empty it renders a token-backed empty
 * state (caller copy per behavior spec §6) instead of a dead select.
 */
export function EntityPickerField({
  id,
  label,
  entries,
  value,
  onChange,
  placeholder,
  required = false,
  disabled = false,
  invalid = false,
  helperText,
  emptyTitle,
  emptyDescription,
  className,
}: EntityPickerFieldProps) {
  const helperId = id ? `${id}-helper` : undefined;

  return (
    <div className={cn('flex flex-col gap-1.5', className)} data-testid="entity-picker-field">
      <Label htmlFor={id}>
        {label} {required && <span className="text-red-700">*</span>}
      </Label>
      {entries.length === 0 ? (
        <div
          data-testid="entity-picker-empty"
          className="rounded-control border border-dashed border-gray-alpha-400 bg-background-200 p-4"
        >
          {emptyTitle && (
            <p className="text-label-14 font-medium text-gray-1000">{emptyTitle}</p>
          )}
          {emptyDescription && (
            <p className="mt-1 text-copy-13 text-gray-700">{emptyDescription}</p>
          )}
        </div>
      ) : (
        <Select
          id={id}
          value={value ?? ''}
          onChange={(event) => onChange(event.target.value)}
          disabled={disabled}
          invalid={invalid}
          aria-describedby={helperText ? helperId : undefined}
          data-testid="entity-picker-select"
        >
          <option value="" disabled>
            {placeholder ?? ''}
          </option>
          {entries.map((entry) => (
            <option key={entry.id} value={entry.id}>
              {entry.subtitle ? `${entry.title} — ${entry.subtitle}` : entry.title}
            </option>
          ))}
        </Select>
      )}
      {helperText && entries.length > 0 && (
        <p id={helperId} className="text-copy-13 text-gray-700">
          {helperText}
        </p>
      )}
    </div>
  );
}
