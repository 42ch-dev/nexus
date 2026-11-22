/**
 * Mirrors apps/web `components/brand/nexus-logo.tsx`: theme-stable square primary
 */

import logoPrimarySquare from '@42ch/nexus-ui/assets/logos/logo-primary-square.svg';
import { NexusLogo, logoShellHeightPx } from '@42ch/nexus-ui';

export function StudioShellLogo({ className }: { className?: string } = {}) {
  return (
    <NexusLogo
      variant="primary"
      src={logoPrimarySquare}
      label="Nexus"
      size={logoShellHeightPx}
      className={className ?? 'h-5 w-auto max-w-full shrink-0'}
    />
  );
}
