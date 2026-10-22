import { useState } from 'react';

import {
  Button,
  Sheet,
  SheetTrigger,
  SheetClose,
  SheetContent,
} from '@42ch/nexus-ui';

/**
 * Studio Components fixture for `<Sheet>` (V1.148 P1 promotion).
 *
 * End-aligned drawer built on Radix dialog — focus trap, Escape dismiss, and
 * return-focus come from Radix; the rail sits at the right edge with a
 * border-l separator. Copy is caller-owned literal English.
 */
export function SheetFixtures() {
  const [open, setOpen] = useState(false);

  return (
    <div data-testid="sheet-fixtures">
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <Sheet open={open} onOpenChange={setOpen}>
          <SheetTrigger asChild>
            <Button variant="primary" data-testid="sheet-open-trigger">
              Open sheet
            </Button>
          </SheetTrigger>
          <SheetContent
            title="Example sheet"
            description="Right-rail drawer for contextual detail and quick edits."
          >
            <div className="flex flex-col gap-4 p-4">
              <p className="text-copy-14 text-gray-900">
                Sheet body content. The overlay dims the workspace; the rail
                stays at the right edge and Escape dismisses back to the
                trigger.
              </p>
              <div className="flex justify-end gap-3">
                <SheetClose asChild>
                  <Button variant="secondary">Close</Button>
                </SheetClose>
                <Button variant="primary" onClick={() => setOpen(false)}>
                  Save
                </Button>
              </div>
            </div>
          </SheetContent>
        </Sheet>
        <p className="text-copy-13 text-gray-700 mt-4">
          Promoted{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @42ch/nexus-ui
          </code>{' '}
          Sheet — end-rail drawer for the work-shell right rail. Keyboard:
          Tab cycles trapped controls; Escape closes and restores focus to
          the trigger.
        </p>
      </div>
    </div>
  );
}
