/**
 * Shell lockup for Design Studio fixtures.
 * Mirrors apps/web `components/brand/nexus-logo.tsx`: theme-stable `logo-primary`
 * square plate lockup (deep-blue plate) for both light and dark. Use
 * `logo-white-bg` only when a light plate is required. Does not read `.dark`
 * or any theme class — the primary plate is constant across themes.
 */

import logoPrimary from '@42ch/nexus-ui/assets/logos/logo-primary.svg';
import { NexusLogo, logoShellHeightPx } from '@42ch/nexus-ui';

export function StudioShellLogo({ className }: { className?: string } = {}) {
  return (
    <NexusLogo
      variant="primary"
      src={logoPrimary}
      label="Nexus"
      size={logoShellHeightPx}
      className={className ?? 'h-5 w-auto max-w-full shrink-0'}
    />
  );
}
