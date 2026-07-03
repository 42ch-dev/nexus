import path from 'node:path';
import { fileURLToPath } from 'node:url';

import sharp from 'sharp';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../../../..');
const logoPath = path.join(repoRoot, 'packages/nexus-ui/assets/logos/logo_light.png');
const out1024 = path.join(__dirname, 'source-1024.png');
const out256 = path.join(__dirname, 'app-icon-preview-256.png');

const CANVAS = 1024;
const PADDING_RATIO = 0.1;
const inner = Math.round(CANVAS * (1 - 2 * PADDING_RATIO));

const trimmed = await sharp(logoPath).trim().toBuffer();
const { width, height } = await sharp(trimmed).metadata();
if (!width || !height) {
  throw new Error(`Could not read logo dimensions from ${logoPath}`);
}

const scale = Math.min(inner / width, inner / height);
const markWidth = Math.round(width * scale);
const markHeight = Math.round(height * scale);
const mark = await sharp(trimmed).resize(markWidth, markHeight).toBuffer();

const composed = await sharp({
  create: {
    width: CANVAS,
    height: CANVAS,
    channels: 4,
    background: { r: 255, g: 255, b: 255, alpha: 1 },
  },
})
  .composite([
    {
      input: mark,
      left: Math.round((CANVAS - markWidth) / 2),
      top: Math.round((CANVAS - markHeight) / 2),
    },
  ])
  .png()
  .toBuffer();

await sharp(composed).toFile(out1024);
await sharp(composed).resize(256, 256).png().toFile(out256);

console.log(`Composed ${out1024} and ${out256} from ${logoPath}`);
