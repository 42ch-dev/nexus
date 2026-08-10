import { type HTMLAttributes, type Ref, type TdHTMLAttributes, type ThHTMLAttributes } from 'react';

import { cn } from '@/lib/utils';

type TableProps = HTMLAttributes<HTMLTableElement> & { ref?: Ref<HTMLTableElement> };
type TableSectionProps = HTMLAttributes<HTMLTableSectionElement> & {
  ref?: Ref<HTMLTableSectionElement>;
};
type TableRowProps = HTMLAttributes<HTMLTableRowElement> & { ref?: Ref<HTMLTableRowElement> };
type TableHeadProps = ThHTMLAttributes<HTMLTableCellElement> & {
  ref?: Ref<HTMLTableCellElement>;
};
type TableCellProps = TdHTMLAttributes<HTMLTableCellElement> & { ref?: Ref<HTMLTableCellElement> };

/**
 * Table primitives — DESIGN.md §Component Primitives/Table.
 *
 * Header: background-200, label-12, gray-900, bottom border gray-alpha-400.
 * Rows: copy-14, primary text gray-1000, secondary gray-900; hover
 * background-200. Use label-12-mono for IDs/cursors. Tables must wrap in an
 * overflow-x container on narrow screens (handled by the screen, not here).
 */
export function Table({ className, ref, ...props }: TableProps) {
  return (
    <div className="w-full overflow-x-auto">
      <table
        ref={ref}
        className={cn('w-full border-collapse text-left text-copy-14', className)}
        {...props}
      />
    </div>
  );
}
Table.displayName = 'Table';

export function TableHeader({ className, ref, ...props }: TableSectionProps) {
  return (
    <thead
      ref={ref}
      className={cn('bg-background-200 text-gray-900', className)}
      {...props}
    />
  );
}
TableHeader.displayName = 'TableHeader';

export function TableBody({ className, ref, ...props }: TableSectionProps) {
  return <tbody ref={ref} className={cn('divide-y divide-gray-alpha-200', className)} {...props} />;
}
TableBody.displayName = 'TableBody';

export function TableRow({ className, ref, ...props }: TableRowProps) {
  return (
    <tr
      ref={ref}
      className={cn('transition-colors duration-state ease-standard motion-reduce:transition-none hover:bg-background-200', className)}
      {...props}
    />
  );
}
TableRow.displayName = 'TableRow';

export function TableHead({ className, ref, ...props }: TableHeadProps) {
  return (
    <th
      ref={ref}
      className={cn('whitespace-nowrap border-b border-gray-alpha-400 px-3 py-2 text-label-12 font-semibold', className)}
      {...props}
    />
  );
}
TableHead.displayName = 'TableHead';

export function TableCell({ className, ref, ...props }: TableCellProps) {
  return <td ref={ref} className={cn('px-3 py-3 align-top text-gray-1000', className)} {...props} />;
}
TableCell.displayName = 'TableCell';
