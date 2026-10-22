import { type ReactNode } from 'react';

import { cn } from '../lib/cn';
import { Label } from './label';

/**
 * SettingsSection — DESIGN.md §Component Primitives (settings-section family).
 *
 * V1.150 P2 — L2 pattern component: the section chrome behind settings
 * surfaces (appearance/connection/workspace/…). Parallel to V1.149
 * FormSection minus the error slot: native fieldset/legend group semantics,
 * caller owns the cards/forms inside and all copy.
 */
export interface SettingsSectionProps {
  /** Section headline copy. */
  title: string;
  /** Helper paragraph copy under the headline. */
  description?: ReactNode;
  /** Cards/forms stay caller-owned. */
  children: ReactNode;
  className?: string;
}

export function SettingsSection({
  title,
  description,
  children,
  className,
}: SettingsSectionProps) {
  return (
    <fieldset className={cn('flex flex-col gap-6', className)}>
      <legend className="text-heading-16 font-heading text-gray-1000">{title}</legend>
      {description && <p className="text-copy-14 text-gray-900">{description}</p>}
      <div className="flex flex-col gap-6">{children}</div>
    </fieldset>
  );
}

/**
 * SettingRow — label/control/hint row inside a SettingsSection card.
 *
 * `htmlFor` wires the promoted Label to the caller's control (the app
 * useId pattern); `hint` renders the helper copy under the control — the
 * caller owns the control's aria-describedby wiring to their hint id.
 */
export interface SettingRowProps {
  /** Control label copy. */
  label: string;
  /** Wires Label → control a11y (appearance-section useId pattern). */
  htmlFor?: string;
  /** Helper copy under the control. */
  hint?: ReactNode;
  /** The control (Select/Input/Button), caller-owned. */
  children: ReactNode;
  className?: string;
}

export function SettingRow({ label, htmlFor, hint, children, className }: SettingRowProps) {
  return (
    <div className={cn('flex flex-col gap-1.5', className)}>
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
      {hint && <p className="text-copy-13 text-gray-700">{hint}</p>}
    </div>
  );
}
