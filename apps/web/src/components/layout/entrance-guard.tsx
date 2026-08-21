import { useEffect, useRef, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Navigate, useLocation } from 'react-router';

import { LoadingState } from '@/components/ui/states';
import {
  ENTRANCE_BY_ID,
  ENTRANCE_ROUTE_RULES,
  matchEntranceRouteRule,
  type EntranceId,
} from '@/components/layout/entrance-registry';
import {
  isSettingsDrivenPath,
  resolveSettingsLocation,
} from '@/components/layout/settings-section-registry';
import { useEntrance } from '@/lib/entrance-context';
import { useToast } from '@/lib/use-toast';

/**
 * Entrance gate (AR-19) — Create bounces develop-only surfaces, Develop never
 * bounces. Query/hash are dropped on the bounce (replace navigation, matches
 * the `StrategyRedirect` discipline). `allowDeepLink` rules pass through with
 * no bounce and no toast (strategy canvas support deep-link).
 */
export function resolveEntranceBounce(
  entrance: EntranceId,
  pathname: string,
  hash: string,
): string | null {
  if (entrance === 'developer') return null; // Develop never bounces

  // Settings-modal paths (`/settings/*`, `/modules`) resolve the section first
  // via the existing machinery and check section-level visibility (AR-19).
  if (isSettingsDrivenPath(pathname)) {
    const resolved = resolveSettingsLocation(pathname, hash);
    if (resolved) {
      const rule = ENTRANCE_ROUTE_RULES.find(
        (r) => r.settingsSection === resolved.section,
      );
      if (rule?.visibility === 'develop-only') {
        return ENTRANCE_BY_ID[entrance].landRoute;
      }
    }
    return null;
  }

  const rule = matchEntranceRouteRule(pathname);
  if (rule?.visibility === 'develop-only' && !rule.allowDeepLink) {
    return ENTRANCE_BY_ID[entrance].landRoute;
  }
  return null;
}

/**
 * Wraps the gated layout tree. Sits inside the existing gate stack
 * (`<SetupGate><EntranceGuard><RootLayout /></EntranceGuard></SetupGate>`);
 * no route is removed or re-registered — the guard only redirects (AR-15/18).
 */
export function EntranceGuard({ children }: { children: ReactNode }) {
  const { entrance, isLoading } = useEntrance();
  const location = useLocation();
  const { t } = useTranslation('shell');
  const { toast } = useToast();
  const lastBouncedKey = useRef<string | null>(null);

  const bounceTarget = resolveEntranceBounce(
    entrance,
    location.pathname,
    location.hash,
  );
  const bounceKey = bounceTarget
    ? `${location.pathname}${location.search}${location.hash}`
    : null;

  // One-shot toast per bounce episode: fired once per location, reset once the
  // guard stops bouncing (redirect landed or the user moved on).
  useEffect(() => {
    if (bounceKey === null) {
      lastBouncedKey.current = null;
      return;
    }
    if (bounceKey === lastBouncedKey.current) return;
    lastBouncedKey.current = bounceKey;
    toast({ variant: 'info', title: t(ENTRANCE_BY_ID[entrance].bounceToastKey) });
  }, [bounceKey, entrance, t, toast]);

  if (isLoading) {
    return <LoadingState label="" />;
  }

  if (entrance === 'developer' || bounceTarget === null) {
    return <>{children}</>;
  }
  return <Navigate to={bounceTarget} replace />;
}

/**
 * Entrance-aware index redirect (AR-18): `content-creator` → `/works`,
 * `developer` → `/developer`. `landRoute` is the single source for guard
 * bounces AND this redirect.
 *
 * First-run (EL §2): browser installs with nothing stored land on the
 * identity page (`/entrance`) once instead of a layout tree — the page's
 * Continue is the only path that persists (AR-20). Desktop first-run is the
 * wizard step (AR-17); returning desktop installs use the footer switch.
 */
export function EntranceIndexRedirect() {
  const { entrance, isLoading, isFirstRun } = useEntrance();
  if (isLoading) return null;
  if (isFirstRun) return <Navigate to="/entrance" replace />;
  return <Navigate to={ENTRANCE_BY_ID[entrance].landRoute} replace />;
}
