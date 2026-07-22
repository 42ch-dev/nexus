import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import sharp from 'sharp';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../../../..');
const designPath = path.join(repoRoot, 'DESIGN.md');
/** Timeline mark — bright gradient for Chronos deep plate (cyan acceptable on deep). */
const logoPath = path.join(repoRoot, 'packages/nexus-ui/assets/logos/logo-color.svg');
const out1024 = path.join(__dirname, 'source-1024.png');
const out256 = path.join(__dirname, 'app-icon-preview-256.png');

const CANVAS = 1024;
const PADDING_RATIO = 0.1;
/** Timeline mark viewBox aspect (284×28). */
const MARK_ASPECT = 284 / 28;
/** Plate fill — DESIGN.md `colors.brand-deep-blue` (Chronos ink). */
const PLATE_TOKEN = 'brand-deep-blue';

/** Drop shadow tuned for a 1024px app-icon canvas on Chronos deep plate. */
const SHADOW = {
  offsetXRatio: 0,
  offsetYRatio: 0.018,
  blurSigmaRatio: 0.008,
  opacity: 0.28,
  /** Gaussian spread padding (× blur sigma) so blur is not clipped at mark edges. */
  padSigmaFactor: 3,
  /** Darker than plate so the halo reads against brand-deep-blue. */
  tintToken: 'brand-deep-blue-1000',
};

/** Read a hex color from root DESIGN.md YAML `colors:` block. */
function designColor(tokenName) {
  const text = readFileSync(designPath, 'utf8');
  const match = text.match(new RegExp(`^\\s*${tokenName}:\\s*"([^"]+)"`, 'm'));
  if (!match) {
    throw new Error(`Missing DESIGN.md colors.${tokenName} in ${designPath}`);
  }
  return match[1];
}

function hexToRgba(hex) {
  const normalized = hex.replace('#', '');
  if (normalized.length !== 6) {
    throw new Error(`Expected 6-digit hex color, got ${hex}`);
  }
  return {
    r: Number.parseInt(normalized.slice(0, 2), 16),
    g: Number.parseInt(normalized.slice(2, 4), 16),
    b: Number.parseInt(normalized.slice(4, 6), 16),
    alpha: 1,
  };
}

async function buildShadowLayer(markBuffer, markWidth, markHeight, tint, config) {
  const blurSigma = Math.max(4, Math.round(markWidth * config.blurSigmaRatio));
  const pad = Math.ceil(blurSigma * config.padSigmaFactor);
  const paddedWidth = markWidth + pad * 2;
  const paddedHeight = markHeight + pad * 2;

  const alpha = await sharp(markBuffer)
    .ensureAlpha()
    .extractChannel('alpha')
    .extend({
      top: pad,
      bottom: pad,
      left: pad,
      right: pad,
      background: { r: 0, g: 0, b: 0, alpha: 0 },
    })
    .blur(blurSigma)
    .linear(config.opacity, 0)
    .raw()
    .toBuffer();

  const pixels = paddedWidth * paddedHeight;
  const rgba = Buffer.alloc(pixels * 4);
  for (let i = 0; i < pixels; i += 1) {
    const offset = i * 4;
    rgba[offset] = tint.r;
    rgba[offset + 1] = tint.g;
    rgba[offset + 2] = tint.b;
    rgba[offset + 3] = alpha[i];
  }

  const buffer = await sharp(rgba, {
    raw: { width: paddedWidth, height: paddedHeight, channels: 4 },
  })
    .png()
    .toBuffer();

  return { buffer, pad };
}

// Chronos app icon: timeline mark (logo-color.svg) on brand-deep-blue plate.
// Plate is opaque Chronos ink (#0D2B3E). macOS applies the system squircle mask.
const plate = hexToRgba(designColor(PLATE_TOKEN));
const shadowTint = hexToRgba(designColor(SHADOW.tintToken));

const inner = Math.round(CANVAS * (1 - 2 * PADDING_RATIO));
// Fit wide timeline mark inside the padded square (width-limited).
const markWidth = inner;
const markHeight = Math.round(markWidth / MARK_ASPECT);

// Rasterize SVG at 2× then downscale for clean edges.
const mark = await sharp(logoPath, { density: 384 })
  .resize(markWidth, markHeight, { fit: 'fill' })
  .png()
  .toBuffer();

const { buffer: shadow, pad: shadowPad } = await buildShadowLayer(
  mark,
  markWidth,
  markHeight,
  shadowTint,
  SHADOW,
);

const markLeft = Math.round((CANVAS - markWidth) / 2);
const markTop = Math.round((CANVAS - markHeight) / 2);
const shadowLeft = markLeft + Math.round(markWidth * SHADOW.offsetXRatio) - shadowPad;
const shadowTop = markTop + Math.round(markHeight * SHADOW.offsetYRatio) - shadowPad;

const composed = await sharp({
  create: {
    width: CANVAS,
    height: CANVAS,
    channels: 4,
    background: plate,
  },
})
  .composite([
    { input: shadow, left: shadowLeft, top: shadowTop },
    { input: mark, left: markLeft, top: markTop },
  ])
  .png()
  .toBuffer();

await sharp(composed).toFile(out1024);
await sharp(composed).resize(256, 256).png().toFile(out256);

console.log(
  `Composed ${out1024} and ${out256} from ${path.relative(repoRoot, logoPath)} on ${PLATE_TOKEN} plate with ${SHADOW.tintToken} shadow (opacity ${SHADOW.opacity})`,
);
