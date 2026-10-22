/**
 * App rail — the translucent dock (IA spec §3.2, prototype visual baseline).
 *
 * A floating, blurred dock of app entries driven by the entrance-scoped
 * registry. Locked items render greyed with the § Copy reason as tooltip and
 * toast (same sentence); they never navigate, so a locked app's `load` factory
 * is never invoked and its route chunk is never requested. Settings lives in
 * the topnav — no bottom-pinned rail item.
 */
import { NavLink, useLocation } from 'react-router';
import { useTranslation } from 'react-i18next';

import { appsForEntrance, isAppLocked, type AppDef } from '@/lib/app-registry';
import { useActivation } from '@/lib/activation-store';
import { useActiveCreatorId } from '@/lib/active-creator-context';
import { useEntrance } from '@/lib/entrance-context';
import { useToast } from '@/lib/use-toast';
import { cn } from '@/lib/utils';

export interface AppRailProps {
  /** Test/Storybook injection seam — defaults to the entrance registry. */
  apps?: readonly AppDef[];
}

export function AppRail({ apps }: AppRailProps) {
  const { t } = useTranslation('shell');
  const { entrance } = useEntrance();
  const activeCreatorId = useActiveCreatorId();
  const { isUnlocked } = useActivation(activeCreatorId);
  const { toast } = useToast();
  const location = useLocation();
  const items = apps ?? appsForEntrance(entrance);

  return (
    <nav
      aria-label={t('aria.rail')}
      data-testid="app-rail"
      className="m-3 hidden w-[72px] shrink-0 flex-col items-center gap-1.5 self-start rounded-card border border-gray-alpha-400 bg-background-100/80 p-3 shadow-elevation-3 backdrop-blur-md lg:flex"
    >
      {items.map((app) => {
        const Icon = app.icon;
        const locked = isAppLocked(app, isUnlocked);

        if (locked) {
          return (
            <button
              key={app.id}
              type="button"
              data-testid={`rail-item-${app.id}`}
              aria-disabled="true"
              title={app.lockReason}
              onClick={() =>
                app.lockReason && toast({ title: app.lockReason, variant: 'info' })
              }
              className="flex w-14 cursor-not-allowed flex-col items-center gap-0.5 rounded-control py-2 text-gray-600 opacity-35"
            >
              <Icon className="h-[22px] w-[22px]" aria-hidden="true" />
              <span className="text-label-12">{t(app.label)}</span>
            </button>
          );
        }

        return (
          <NavLink
            key={app.id}
            to={app.route}
            data-testid={`rail-item-${app.id}`}
            title={t(app.label)}
            className={({ isActive }) =>
              cn(
                'flex w-14 flex-col items-center gap-0.5 rounded-control py-2 text-gray-800 transition-[background-color,color] duration-state ease-standard hover:bg-gray-alpha-100 motion-reduce:transition-none',
                (isActive || location.pathname.startsWith(`${app.route}/`)) &&
                  'bg-gray-alpha-100 text-blue-700',
              )
            }
          >
            <Icon className="h-[22px] w-[22px]" aria-hidden="true" />
            <span className="text-label-12">{t(app.label)}</span>
          </NavLink>
        );
      })}
    </nav>
  );
}
