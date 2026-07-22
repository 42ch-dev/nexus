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
/** Transparent inset per side (~12% of canvas; matches Studio VI-004 squircle fixture). */
const INSET_RATIO = 12 / 96;
const insetPx = Math.round(CANVAS * INSET_RATIO);
const innerSize = CANVAS - insetPx * 2;

// Rasterize the primary plate lockup smaller, then pad with transparent margins so
// macOS squircle masking does not clip plate corners into a light rectangular halo.
const plateRaster = await sharp(logoPath, { density: 384 })
  .resize(innerSize, innerSize, { fit: 'fill' })
  .png()
  .toBuffer();

const composed = await sharp({
  create: {
    width: CANVAS,
    height: CANVAS,
    channels: 4,
    background: { r: 0, g: 0, b: 0, alpha: 0 },
  },
})
  .composite([{ input: plateRaster, top: insetPx, left: insetPx }])
  .png()
  .toBuffer();

await sharp(composed).toFile(out1024);
await sharp(composed).resize(256, 256).png().toFile(out256);

console.log(
  `Composed ${out1024} and ${out256} from ${path.relative(repoRoot, logoPath)}`,
);
