import logoColor from '@42ch/nexus-ui/assets/logos/logo-color.svg';
import logoPrimary from '@42ch/nexus-ui/assets/logos/logo-primary.svg';
import { NexusLogo as NexusLogoComponent } from '@42ch/nexus-ui';

import { useTheme } from '@/components/theme-provider';
import { cn } from '@/lib/utils';

export interface NexusLogoProps {
  /** Accessible label for the mark. Defaults to the product name. */
  label?: string;
  className?: string;
}

/**
 * Theme-aware Nexus wordmark — thin wrapper around `@42ch/nexus-ui`.
 *
 * Resolves the SVG asset via Vite and maps the current theme to the canonical
 * package variant, preserving the zero-prop call-site ergonomics in
 * `sidebar.tsx` and `header.tsx`.
 */
export function NexusLogo({ label = 'Nexus', className }: NexusLogoProps) {
  const { resolvedTheme } = useTheme();
  const variant = resolvedTheme === 'dark' ? 'color' : 'primary';
  const src = variant === 'color' ? logoColor : logoPrimary;

  return (
    <NexusLogoComponent
      variant={variant}
      src={src}
      label={label}
      size={32}
      className={cn('h-8 w-auto shrink-0', className)}
    />
  );
}
