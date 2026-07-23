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
/**
 * Opaque margin between canvas edge and the rounded plate (not alpha).
 * Peer Dock icons keep artwork inset; full-bleed square plates read as sharp squares.
 */
const PLATE_INSET_RATIO = 0.06;
/** macOS squircle corner radius — matches Studio VI-004 `rounded-[22%]`. */
const SQUIRCLE_RADIUS_RATIO = 0.22;

const inset = Math.round(CANVAS * PLATE_INSET_RATIO);
const plateSize = CANVAS - 2 * inset;
const cornerRadius = Math.round(plateSize * SQUIRCLE_RADIUS_RATIO);

const plateRaster = await sharp(logoPath, { density: 384 })
  .resize(plateSize, plateSize, { fit: 'fill' })
  .flatten({ background: PLATE_COLOR })
  .png()
  .toBuffer();

const squircleMaskSvg = Buffer.from(
  `<svg width="${plateSize}" height="${plateSize}" xmlns="http://www.w3.org/2000/svg">
    <rect width="${plateSize}" height="${plateSize}" rx="${cornerRadius}" ry="${cornerRadius}" fill="white"/>
  </svg>`,
);
const squircleMaskPng = await sharp(squircleMaskSvg).png().toBuffer();

// Bake visible squircle rounding: clip the plate to a rounded rect, then flatten
// clipped pixels back to plate color so the output stays fully opaque (H1 baseline).
const roundedPlate = await sharp(plateRaster)
  .composite([{ input: squircleMaskPng, blend: 'dest-in' }])
  .flatten({ background: PLATE_COLOR })
  .png()
  .toBuffer();

// Center the pre-rounded plate on an opaque full-canvas plate background.
const composed = await sharp({
  create: {
    width: CANVAS,
    height: CANVAS,
    channels: 3,
    background: PLATE_COLOR,
  },
})
  .composite([{ input: roundedPlate, left: inset, top: inset }])
  .png()
  .toBuffer();

await sharp(composed).removeAlpha().png().toFile(out1024);
await sharp(composed).removeAlpha().resize(256, 256).png().toFile(out256);

console.log(
  `Composed ${out1024} and ${out256} from ${path.relative(repoRoot, logoPath)} ` +
    `(squircle plate inset=${inset}px radius=${cornerRadius}px)`,
);
