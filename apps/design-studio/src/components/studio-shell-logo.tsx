/**
 * Shell lockup for Design Studio fixtures.
 * Mirrors apps/web `components/brand/nexus-logo.tsx`: default `logo-primary`
 * (deep-blue plate) for both themes. Use `logo-white-bg` only when a light
 * plate is required.
 *
 * Reads the document `.dark` class (kept in sync by Studio ThemeProvider) so
 * fixtures stay theme-aware even though the primary lockup does not switch.
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
