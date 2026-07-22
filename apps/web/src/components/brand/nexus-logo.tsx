import logoPrimarySquare from '@42ch/nexus-ui/assets/logos/logo-primary-square.svg';
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
 * Shell chrome uses the square primary plate (`logo-primary-square.svg`) at
 * {@link logoShellHeightPx}. Plain wide marks (`logo-primary.svg`) are for
 * inline timeline usage — not the default sidebar/header plate lockup.
 */
export function NexusLogo({ label = 'Nexus', className }: NexusLogoProps) {
  return (
    <NexusLogoComponent
      variant="primary"
      src={logoPrimarySquare}
      label={label}
      size={logoShellHeightPx}
      className={cn('h-5 w-auto max-w-full shrink-0', className)}
    />
  );
}
