import { useCallback, useEffect, useRef, useState } from 'react';

import type { HubTab } from '@/components/layout/presentational/hub-tab-bar';

export function resolveInitialHubTab(worldCount: number, workCount: number): HubTab {
  if (worldCount > 0) return 'world';
  if (workCount > 0) return 'work';
  return 'world';
}

/**
 * Hub tab SSOT with one-shot auto-resolution after list queries hydrate (V1.134 P3 IA §1.2).
 * Auto-switch stops once the author manually changes tabs.
 */
export function useHubTabState(
  worldCount: number,
  workCount: number,
  isListsLoading: boolean,
): {
  activeTab: HubTab;
  onTabChange: (tab: HubTab) => void;
} {
  const [activeTab, setActiveTab] = useState<HubTab>('world');
  const hasUserChangedTab = useRef(false);
  const hasAutoResolvedInitialTab = useRef(false);

  useEffect(() => {
    if (isListsLoading) return;
    if (hasUserChangedTab.current || hasAutoResolvedInitialTab.current) return;

    setActiveTab(resolveInitialHubTab(worldCount, workCount));
    hasAutoResolvedInitialTab.current = true;
  }, [isListsLoading, worldCount, workCount]);

  const onTabChange = useCallback((tab: HubTab) => {
    hasUserChangedTab.current = true;
    setActiveTab(tab);
  }, []);

  return { activeTab, onTabChange };
}
