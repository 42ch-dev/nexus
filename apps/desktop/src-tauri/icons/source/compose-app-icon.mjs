import path from 'node:path';
import { fileURLToPath } from 'node:url';

import sharp from 'sharp';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../../../..');
/** Primary lockup already includes the Chronos deep plate + bright mark. */
const logoPath = path.join(repoRoot, 'packages/nexus-ui/assets/logos/logo-primary.svg');
const out1024 = path.join(__dirname, 'source-1024.png');
const out256 = path.join(__dirname, 'app-icon-preview-256.png');

const CANVAS = 1024;

// Rasterize the primary plate lockup to the app-icon canvas.
// macOS applies the system squircle mask to the bundled asset.
const composed = await sharp(logoPath, { density: 384 })
  .resize(CANVAS, CANVAS, { fit: 'fill' })
  .png()
  .toBuffer();

await sharp(composed).toFile(out1024);
await sharp(composed).resize(256, 256).png().toFile(out256);

console.log(
  `Composed ${out1024} and ${out256} from ${path.relative(repoRoot, logoPath)}`,
);
