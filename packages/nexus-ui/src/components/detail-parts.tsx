import { type ReactNode } from 'react';

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from './card';
import { cn } from '../lib/cn';

/**
 * FieldRow — DESIGN.md §Component Primitives/detail-parts family.
 *
 * V1.150 P3 — read-only detail row (label/value pair) behind the app-side
 * work-timeline-inspector / timeline-moment-inspector / modules-page
 * DetailField shapes. Renders nothing when the value is null/undefined/''
 * (shared null-hide behavior) so callers can map rows over sparse data
 * without guarding each row. Pure presentational: caller owns the copy.
 */
export interface FieldRowProps {
  /** Row label copy. */
  label: string;
  /** Row value copy; null/undefined/'' hides the row. */
  value?: ReactNode;
  className?: string;
}

export function FieldRow({ label, value, className }: FieldRowProps) {
  if (value === null || value === undefined || value === '') {
    return null;
  }

  return (
    <div className={cn('flex items-baseline justify-between gap-4', className)}>
      <span className="text-copy-13 text-gray-700">{label}</span>
      <span className="text-copy-14 text-right text-gray-1000">{value}</span>
    </div>
  );
}

/**
 * DetailCard — read-only detail panel chrome (findings-page detail Card
 * shape): the promoted Card primitives with caller-owned title/description
 * copy and FieldRow groups or any caller content in the body.
 */
export interface DetailCardProps {
  /** Panel headline copy (CardTitle). */
  title: string;
  /** Panel helper copy (CardDescription). */
  description?: ReactNode;
  /** FieldRow groups / caller content. */
  children: ReactNode;
  className?: string;
}

export function DetailCard({ title, description, children, className }: DetailCardProps) {
  return (
    <Card className={className}>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        {description && <CardDescription>{description}</CardDescription>}
      </CardHeader>
      <CardContent className="flex flex-col gap-3">{children}</CardContent>
    </Card>
  );
}
