import { type ReactNode } from 'react';

import { cn } from '../lib/cn';

/**
 * FormSection — DESIGN.md §Component Primitives/FormSection.
 *
 * V1.149 P1 — L2 pattern component: the field-group layout behind form
 * dialogs and settings surfaces. Native fieldset/legend group semantics;
 * caller owns the field controls, validation attributes, and all copy.
 * Optional `error` renders the shared inline error slot (role=alert +
 * error-surface tokens) below the description, above the fields.
 */
export interface FormSectionProps {
  title: string;
  description?: ReactNode;
  /** Inline error copy for the whole group; renders the error-surface slot. */
  error?: ReactNode;
  children: ReactNode;
  className?: string;
}

export function FormSection({ title, description, error, children, className }: FormSectionProps) {
  return (
    <fieldset className={cn('flex flex-col gap-4', className)}>
      <legend className="text-heading-16 font-heading tracking-tight text-gray-1000">
        {title}
      </legend>
      {description && <p className="text-copy-14 text-gray-900">{description}</p>}
      {error && (
        <div
          role="alert"
          className="rounded-card border border-error-surface-border bg-error-surface p-3 text-copy-14 text-red-900"
        >
          {error}
        </div>
      )}
      <div className="flex flex-col gap-4">{children}</div>
    </fieldset>
  );
}
