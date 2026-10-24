/**
 * Hub card — the read-only frame for one 操作台 status section (plan v1220 T1).
 *
 * A card carries a title, an optional status pill, its data body, and at most
 * one navigation link. The Hub aggregates state only: no form, no mutation —
 * every action lives in the card's rail app (IA §6).
 */
import type { ReactNode } from 'react';
import { Link } from 'react-router';

import { Badge, type BadgeProps } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';

export interface HubCardProps {
  /** Stable test-id stem (`service` → `hub-card-service`). */
  id: string;
  /** Title text, or a node when the owning surface needs inline markup. */
  title: ReactNode;
  /** Status pill in the header (e.g. 运行中 / 已连接). */
  status?: {
    label: string;
    variant: BadgeProps['variant'];
  };
  /** Navigation-only link into the owning rail app. */
  go?: { to: string; label: string };
  children?: ReactNode;
}

export function HubCard({ id, title, status, go, children }: HubCardProps) {
  return (
    <Card className="flex flex-col" data-testid={`hub-card-${id}`}>
      <CardHeader className="flex-1">
        <div className="flex items-start justify-between gap-2">
          <CardTitle className="text-copy-13">{title}</CardTitle>
          {status && (
            <Badge variant={status.variant} tone="soft">
              {status.label}
            </Badge>
          )}
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-1">
        {children}
        {go && (
          <Link
            to={go.to}
            className="text-copy-13 text-blue-800 transition-colors duration-state ease-standard motion-reduce:transition-none hover:text-blue-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700"
            data-testid={`hub-card-${id}-go`}
          >
            {go.label}
          </Link>
        )}
      </CardContent>
    </Card>
  );
}
