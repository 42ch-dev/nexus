import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Outlet } from 'react-router-dom';
import { PanelRight } from 'lucide-react';

import { WorkRail } from '@/components/layout/work-rail';
import { Button } from '@/components/ui/button';
import { Sheet, SheetContent } from '@/components/ui/sheet';
import { useMinLgViewport } from '@/lib/use-min-lg-viewport';

/**
 * Canvas-first work shell — flex main outlet + right {@link WorkRail}.
 *
 * Nested inside `RootLayout` `<main>` on `/works/:workId/*` routes (T2 wires
 * nesting). At `lg`+ the rail is a fixed 280px column; below `lg` it collapses
 * to an end-sheet drawer opened from the shell header control.
 */
export function WorkShellLayout() {
  const { t } = useTranslation('shell');
  const [drawerOpen, setDrawerOpen] = useState(false);
  const isDesktopRail = useMinLgViewport();

  const rail = (
    <WorkRail
      showHeader={isDesktopRail}
      onWorkSelect={isDesktopRail ? undefined : () => setDrawerOpen(false)}
    />
  );

  return (
    <div
      className="flex min-h-0 flex-1 flex-col lg:flex-row"
      data-testid="work-shell-layout"
    >
      <div className="flex items-center justify-end border-b border-gray-alpha-400 px-4 py-2 lg:hidden">
        <Button
          type="button"
          variant="secondary"
          size="small"
          data-testid="work-shell-open-rail"
          aria-label={t('workShell.openRailAria')}
          aria-expanded={drawerOpen}
          aria-haspopup="dialog"
          onClick={() => setDrawerOpen(true)}
        >
          <PanelRight className="mr-1.5 h-4 w-4" aria-hidden />
          {t('workShell.openRail')}
        </Button>
      </div>

      <div className="min-h-0 min-w-0 flex-1" data-testid="work-shell-main">
        <Outlet />
      </div>

      <aside
        className="hidden h-full w-[280px] shrink-0 flex-col border-l border-gray-alpha-400 bg-background-100 lg:flex"
        aria-label={t('workShell.railAria')}
        data-testid="work-shell-rail-desktop"
      >
        {isDesktopRail ? rail : null}
      </aside>

      {!isDesktopRail ? (
        <Sheet open={drawerOpen} onOpenChange={setDrawerOpen}>
          <SheetContent title={t('workShell.railTitle')} description={t('workShell.railDrawerDescription')}>
            {drawerOpen ? rail : null}
          </SheetContent>
        </Sheet>
      ) : null}
    </div>
  );
}
