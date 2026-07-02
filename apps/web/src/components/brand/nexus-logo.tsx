import logoColor from '@42ch/nexus-ui/assets/logos/logo-color.svg';
import logoPrimary from '@42ch/nexus-ui/assets/logos/logo-primary.svg';

import { useTheme } from '@/components/theme-provider';
import { cn } from '@/lib/utils';

export interface NexusLogoProps {
  /** Accessible label for the mark. Defaults to the product name. */
  label?: string;
  className?: string;
}

/**
 * Theme-aware Nexus wordmark — root DESIGN.md § Logo Usage.
 * Light shell surfaces use the deep-blue mark; dark chrome uses the cyan mark.
 */
export function NexusLogo({ label = 'Nexus', className }: NexusLogoProps) {
  const { theme } = useTheme();
  const src = theme === 'dark' ? logoColor : logoPrimary;

  return (
    <img
      src={src}
      alt={label}
      width={120}
      height={32}
      className={cn('h-8 w-auto shrink-0', className)}
      decoding="async"
    />
  );
}
