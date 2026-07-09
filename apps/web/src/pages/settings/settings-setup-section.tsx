/**
 * Settings Setup section — V1.103 P0 placeholder.
 *
 * Route resolves under SettingsShellLayout. Re-run Setup confirm dialog
 * lands in P3 — empty frame only for P0 shell wiring.
 */

import { RotateCcw } from 'lucide-react';

export function SettingsSetupSection() {
  return (
    <div
      className="flex flex-col items-center justify-center gap-3 min-h-[200px] rounded-card border border-dashed border-gray-alpha-400 bg-background-100 px-6 py-10 text-center"
      data-testid="settings-setup-section"
    >
      <RotateCcw className="size-8 text-gray-500" aria-hidden="true" />
      <p className="text-heading-16 font-heading text-gray-1000">Setup</p>
      <p className="text-copy-13 text-gray-700 max-w-sm">
        Re-run Setup will appear here (P3).
      </p>
    </div>
  );
}
