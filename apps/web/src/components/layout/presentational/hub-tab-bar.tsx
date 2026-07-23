import { cn } from '@/lib/utils';

export type HubTab = 'world' | 'work';

export type HubTabBarLabels = {
  world: string;
  work: string;
};

export type HubTabBarProps = {
  activeTab: HubTab;
  onTabChange: (tab: HubTab) => void;
  labels: HubTabBarLabels;
  ariaLabel?: string;
  'data-testid'?: string;
};

const TABS: HubTab[] = ['world', 'work'];

/**
 * Shared World / Work tab bar for Creator Hub dual-pane IA (V1.134 P3).
 *
 * Presentational extract consumed by App hub routes and Design Studio
 * fixtures via `@web-layout/hub-tab-bar`. Host owns tab SSOT and i18n labels.
 */
export function HubTabBar({
  activeTab,
  onTabChange,
  labels,
  ariaLabel = 'Creator hub entity kind',
  'data-testid': testId = 'hub-tab-bar',
}: HubTabBarProps) {
  return (
    <div
      className="border-b border-gray-alpha-400 px-4"
      data-testid={testId}
    >
      <div
        className="flex gap-1"
        role="tablist"
        aria-label={ariaLabel}
      >
        {TABS.map((tab) => {
          const active = activeTab === tab;
          const label = tab === 'world' ? labels.world : labels.work;

          return (
            <button
              key={tab}
              type="button"
              role="tab"
              id={`hub-tab-${tab}`}
              aria-selected={active}
              aria-controls={`hub-tabpanel-${tab}`}
              data-testid={`${testId}-${tab}`}
              onClick={() => onTabChange(tab)}
              className={cn(
                'relative px-4 py-3 text-label-14 font-medium transition-colors duration-state ease-standard motion-reduce:transition-none',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2',
                active
                  ? 'text-gray-1000 after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:bg-brand-cyan after:content-[""]'
                  : 'text-gray-700 hover:text-gray-1000',
              )}
            >
              {label}
            </button>
          );
        })}
      </div>
    </div>
  );
}
