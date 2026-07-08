/**
 * @42ch/nexus-ui — brand assets, tokens, theme helpers, React brand components,
 * and V1.99-approved pure presentational primitives.
 * V1.87 ships: `<NexusLogo>` and `<NexusMark>`.
 * V1.99 P0 ships: `<Button>`, `<Badge>`, `<Card>` (+ sub-primitives).
 */

export {
  brandColors,
  logoClearSpaceRatio,
  logoMinSizePx,
  logoVariants,
  type BrandColorName,
  type LogoVariantName,
} from './tokens';

export { NexusLogo, VARIANT_FILENAMES, type Variant, type NexusLogoProps } from './components/nexus-logo';
export { NexusMark, type NexusMarkProps } from './components/nexus-mark';

// V1.99 P0 — promoted presentational primitives
export { Button, type ButtonProps } from './components/button';
export { Badge, type BadgeProps } from './components/badge';
export {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from './components/card';
