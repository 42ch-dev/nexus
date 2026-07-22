import logoWhite from '@42ch/nexus-ui/assets/logos/logo-white.svg';
import { NexusLogo as NexusLogoComponent, logoShellHeightPx } from '@42ch/nexus-ui';

import { cn } from '@/lib/utils';

export interface NexusInkLogoProps {
  /** Accessible label for the mark. Defaults to the product name. */
  label?: string;
  className?: string;
}

/**
 * Bright mark for ink surfaces (Chronos titlebar).
 * Uses `logo-white.svg` — not the deep `primary` plate.
 */
export function NexusInkLogo({ label = 'Nexus', className }: NexusInkLogoProps) {
  return (
    <NexusLogoComponent
      variant="white"
      src={logoWhite}
      label={label}
      size={logoShellHeightPx}
      draggable={false}
      className={cn('h-5 w-auto max-w-full shrink-0', className)}
    />
  );
}
