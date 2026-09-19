#!/usr/bin/env node

import { execFileSync } from 'node:child_process';
import { mkdirSync, rmSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import sharp from 'sharp';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(scriptDir, '..');
const repoRoot = path.resolve(appRoot, '../..');
const logoPath = path.join(repoRoot, 'packages/nexus-ui/assets/logos/logo-primary-square.svg');
const iconDir = path.join(appRoot, 'resources/icons');
const sourcePath = path.join(iconDir, 'source-1024.png');
const previewPath = path.join(iconDir, 'app-icon-preview-256.png');
const icnsPath = path.join(iconDir, 'app.icns');

const CANVAS = 1024;
const PLATE_COLOR = '#0D2B3E';
const MARGIN_COLOR = '#1A4A66';
const PLATE_INSET_RATIO = 0.06;
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
  `<svg width="${plateSize}" height="${plateSize}" xmlns="http://www.w3.org/2000/svg">` +
    `<rect width="${plateSize}" height="${plateSize}" rx="${cornerRadius}" ry="${cornerRadius}" fill="white"/>` +
  '</svg>',
);
const squircleMaskPng = await sharp(squircleMaskSvg).png().toBuffer();
const roundedPlate = await sharp(plateRaster)
  .composite([{ input: squircleMaskPng, blend: 'dest-in' }])
  .flatten({ background: MARGIN_COLOR })
  .png()
  .toBuffer();

const composed = await sharp({
  create: {
    width: CANVAS,
    height: CANVAS,
    channels: 3,
    background: MARGIN_COLOR,
  },
})
  .composite([{ input: roundedPlate, left: inset, top: inset }])
  .removeAlpha()
  .png()
  .toBuffer();

await sharp(composed).toFile(sourcePath);
await sharp(composed).resize(256, 256).toFile(previewPath);

const iconsetDir = path.join(iconDir, '.app.iconset');
rmSync(iconsetDir, { recursive: true, force: true });
mkdirSync(iconsetDir, { recursive: true });
try {
  for (const [size, scale] of [
    [16, 1],
    [16, 2],
    [32, 1],
    [32, 2],
    [128, 1],
    [128, 2],
    [256, 1],
    [256, 2],
    [512, 1],
    [512, 2],
  ]) {
    const outputName = `icon_${size}x${size}${scale === 2 ? '@2x' : ''}.png`;
    execFileSync('/usr/bin/sips', ['-z', String(size * scale), String(size * scale), sourcePath, '--out', path.join(iconsetDir, outputName)], {
      stdio: 'ignore',
    });
  }
  execFileSync('/usr/bin/iconutil', ['--convert', 'icns', '--output', icnsPath, iconsetDir], { stdio: 'inherit' });
} finally {
  rmSync(iconsetDir, { recursive: true, force: true });
}

console.log(
  `Composed ${path.relative(repoRoot, sourcePath)}, ${path.relative(repoRoot, previewPath)}, and ` +
    `${path.relative(repoRoot, icnsPath)} from ${path.relative(repoRoot, logoPath)} ` +
    `(squircle plate inset=${inset}px radius=${cornerRadius}px)`,
);
