import { useState } from 'react';

import {
  Button,
  Dialog,
  DialogTrigger,
  DialogContent,
} from '@42ch/nexus-ui';

/**
 * Studio Components fixture for `<Dialog>` (V1.148 P1 promotion).
 *
 * Uses the promoted `@42ch/nexus-ui` Dialog primitives (Radix dialog). Radix
 * provides the portal, focus trap, Escape dismiss, and scroll lock; the
 * overlay uses the `bg-scrim` token and the panel `shadow-elevation-4`
 * (V1.121 scrim convergence). Copy is caller-owned literal English.
 */
export function DialogFixtures() {
  const [open, setOpen] = useState(false);

  return (
    <div>
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <Dialog open={open} onOpenChange={setOpen}>
          <DialogTrigger asChild>
            <Button variant="primary" data-testid="dialog-open-trigger">
              Open dialog
            </Button>
          </DialogTrigger>
          <DialogContent
            title="Example dialog"
            description="This dialog demonstrates the title, description, and action pattern."
          >
            <div className="flex flex-col gap-4">
              <p className="text-copy-14 text-gray-900">
                Dialog body content. The overlay dims the background and focus
                is trapped inside the modal until dismissed.
              </p>
              <div className="flex justify-end gap-3">
                <Button variant="secondary" onClick={() => setOpen(false)}>
                  Cancel
                </Button>
                <Button variant="primary" onClick={() => setOpen(false)}>
                  Confirm
                </Button>
              </div>
            </div>
          </DialogContent>
        </Dialog>
        <p className="text-copy-13 text-gray-700 mt-4">
          Promoted{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @42ch/nexus-ui
          </code>{' '}
          Dialog — Radix portal, focus trap, and scroll lock. Keyboard: Tab
          cycles trapped controls; Escape closes and restores focus to the
          trigger.
        </p>
      </div>
    </div>
  );
}
