/**
 * Entrance-scoped app registry (plan v1219 Task 2 — Interfaces).
 *
 * One rail item = one app = one route-level lazy chunk. `load` factories are
 * dynamic imports resolved through `React.lazy` at the route boundary
 * (precedent: the React.lazy route splits in `App.tsx`); a locked app's
 * factory must never be invoked, so its chunk is never requested. `label`
 * carries the `shell` i18n key rendered by the rail.
 *
 * `lockedUntil` exists only for 作品 (`brief-saved` → activation store);
 * 会话/记忆 carry their § Copy lock reason as literal copy with NO
 * `lockedUntil` member and NO unlock path this iteration (compass D17/D26).
 */
import type { ComponentType } from 'react';
import {
  Boxes,
  BrainCircuit,
  Cpu,
  Globe,
  Layers,
  LayoutDashboard,
  Play,
  Workflow,
} from 'lucide-react';

import type { EntranceId } from '@/components/layout/entrance-registry';

export type AppId =
  | 'world'
  | 'work'
  | 'session'
  | 'memory'
  | 'hub'
  | 'harness'
  | 'modules'
  | 'caps';

export interface AppDef {
  id: AppId;
  /** `shell` namespace i18n key for the rail label. */
  label: string;
  icon: ComponentType<{ className?: string }>;
  route: string;
  /** Route-level chunk; invoked only when the app is unlocked and rendered. */
  load: () => Promise<{ default: ComponentType }>;
  /** 作品 only — mapped to the activation store's `brief-saved` event (D26). */
  lockedUntil?: 'brief-saved';
  /** Locked reason, verbatim from the plan § Copy table. Absent = never locked. */
  lockReason?: string;
}

/** Rail lock reasons — plan § Copy (final, product-owned), verbatim. */
export const RAIL_LOCK_REASON = {
  work: '完成 Brief 后解锁「作品」',
  session: '创建作品后解锁「会话」',
  memory: '世界开始运转后解锁「记忆」',
} as const;

const CREATOR_APPS: readonly AppDef[] = [
  {
    id: 'world',
    label: 'rail.world',
    icon: Globe,
    route: '/worlds',
    load: () =>
      import('@/pages/creator-hub-page').then((m) => ({ default: m.CreatorHubPage })),
  },
  {
    id: 'work',
    label: 'rail.work',
    icon: Layers,
    route: '/works',
    load: () =>
      import('@/pages/creator-hub-page').then((m) => ({ default: m.CreatorHubPage })),
    lockedUntil: 'brief-saved',
    lockReason: RAIL_LOCK_REASON.work,
  },
  {
    id: 'session',
    label: 'rail.session',
    icon: Play,
    route: '/sessions',
    load: () => import('@/pages/sessions-page').then((m) => ({ default: m.SessionsPage })),
    lockReason: RAIL_LOCK_REASON.session,
  },
  {
    id: 'memory',
    label: 'rail.memory',
    icon: BrainCircuit,
    route: '/memory',
    load: () => import('@/pages/memory-page').then((m) => ({ default: m.MemoryPage })),
    lockReason: RAIL_LOCK_REASON.memory,
  },
];

const DEVELOPER_APPS: readonly AppDef[] = [
  {
    id: 'hub',
    label: 'rail.hub',
    icon: LayoutDashboard,
    route: '/developer',
    load: () =>
      import('@/components/developer/hub-page').then((m) => ({ default: m.HubPage })),
  },
  {
    id: 'harness',
    label: 'rail.harness',
    icon: Workflow,
    route: '/strategies',
    load: () => import('@/pages/strategies-page').then((m) => ({ default: m.StrategiesPage })),
  },
  {
    id: 'modules',
    label: 'rail.modules',
    icon: Boxes,
    route: '/modules',
    load: () => import('@/pages/modules-page').then((m) => ({ default: m.ModulesPage })),
  },
  {
    id: 'caps',
    label: 'rail.caps',
    icon: Cpu,
    route: '/capabilities',
    load: () =>
      import('@/pages/capabilities-page').then((m) => ({ default: m.CapabilitiesPage })),
  },
  {
    id: 'session',
    label: 'rail.session',
    icon: Play,
    route: '/sessions',
    load: () => import('@/pages/sessions-page').then((m) => ({ default: m.SessionsPage })),
  },
];

export function appsForEntrance(entrance: EntranceId): readonly AppDef[] {
  return entrance === 'developer' ? DEVELOPER_APPS : CREATOR_APPS;
}

/**
 * Locked-state resolution. `brief-saved` apps consult the activation store;
 * apps with literal copy (会话/记忆) stay locked with no unlock path (D17).
 */
export function isAppLocked(app: AppDef, unlocked: (appId: AppId) => boolean): boolean {
  if (!app.lockReason) return false;
  if (app.lockedUntil === 'brief-saved') return !unlocked(app.id);
  return true;
}
