import path from 'node:path';
import { fileURLToPath } from 'node:url';

import sharp from 'sharp';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../../../..');
/** Primary lockup already includes the Chronos deep plate + bright mark. */
const logoPath = path.join(repoRoot, 'packages/nexus-ui/assets/logos/logo-primary-square.svg');
const out1024 = path.join(__dirname, 'source-1024.png');
const out256 = path.join(__dirname, 'app-icon-preview-256.png');

const CANVAS = 1024;
/** Chronos deep plate — matches logo-primary-square.svg rect fill. */
const PLATE_COLOR = '#0D2B3E';

// Opaque full-bleed rule: rasterize the plate lockup edge-to-edge on a 1024×1024
// canvas with no transparent margins. macOS applies its squircle mask to opaque
// full-canvas icons; transparent alpha borders defeat the mask and yield a flat
// square Dock tile instead of the system-rounded squircle.
const composed = await sharp(logoPath, { density: 384 })
  .resize(CANVAS, CANVAS, { fit: 'fill' })
  .flatten({ background: PLATE_COLOR })
  .png()
  .toBuffer();

await sharp(composed).toFile(out1024);
await sharp(composed).resize(256, 256).png().toFile(out256);

console.log(
  `Composed ${out1024} and ${out256} from ${path.relative(repoRoot, logoPath)}`,
);
