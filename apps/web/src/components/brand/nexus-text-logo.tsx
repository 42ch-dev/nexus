import logoText from '@42ch/nexus-ui/assets/logos/logo-text.svg';
import { NexusLogo as NexusLogoComponent, logoMinSizePx } from '@42ch/nexus-ui';

import { cn } from '@/lib/utils';

export interface NexusTextLogoProps {
  /** Accessible label. Defaults to the product name. */
  label?: string;
  className?: string;
  /**
   * Rendered glyph-box height in px. Defaults to {@link logoMinSizePx} (24).
   * Width stays auto so the wordmark keeps its intrinsic ratio.
   */
  size?: number;
}

/**
 * Product wordmark — always `logo-text.svg` via `<NexusLogo variant="text">`.
 *
 * Prefer this (or the package primitive with `variant="text"` + resolved
 * `logo-text.svg`) whenever UI needs the Nexus **logo text**. Do not typeset
 * "nexus" / "Nexus" with UI fonts as a brand substitute — that drifts glyph
 * metrics and weight. Shell plate / ink marks stay on `NexusLogo` /
 * `NexusInkLogo`; route titles and nav labels remain ordinary typography.
 */
export function NexusTextLogo({
  label = 'Nexus',
  className,
  size = logoMinSizePx,
}: NexusTextLogoProps) {
  return (
    <NexusLogoComponent
      variant="text"
      src={logoText}
      label={label}
      size={size}
      className={cn('w-auto max-w-full shrink-0', className)}
    />
  );
}
