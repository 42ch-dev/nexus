import logoPrimary from '@42ch/nexus-ui/assets/logos/logo-primary.svg';
import { NexusLogo as NexusLogoComponent, logoShellHeightPx } from '@42ch/nexus-ui';

import { cn } from '@/lib/utils';

export interface NexusLogoProps {
  /** Accessible label for the mark. Defaults to the product name. */
  label?: string;
  className?: string;
}

/**
 * Product-shell Nexus lockup — thin wrapper around `@42ch/nexus-ui`.
 *
 * Default brand asset is `logo-primary.svg` (bright mark on brand deep-blue
 * plate) for both themes. Use `logo-white-bg.svg` only on surfaces that must
 * sit on a light/white plate. Resolves the SVG via Vite and preserves
 * zero-prop call sites in `sidebar.tsx` / `header.tsx`.
 */
export function NexusLogo({ label = 'Nexus', className }: NexusLogoProps) {
  return (
    <NexusLogoComponent
      variant="primary"
      src={logoPrimary}
      label={label}
      size={logoShellHeightPx}
      className={cn('h-5 w-auto max-w-full shrink-0', className)}
    />
  );
}
