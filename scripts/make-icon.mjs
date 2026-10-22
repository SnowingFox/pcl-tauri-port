#!/usr/bin/env node
// Generates the 1024x1024 source app icon (original artwork, no third-party assets).
//
//   node scripts/make-icon.mjs            -> src-tauri/icons/source-icon.png
//   npx tauri icon src-tauri/icons/source-icon.png   (regenerates icns/ico/png set)
//
// Design: a macOS-style squircle with a diagonal indigo-to-blue gradient, three white
// rounded "blocks" in a 2x2 grid and a four-point gold spark where the fourth block would be.
// Dependency-free: rasterised with 4x4 supersampling, encoded with node:zlib.

import { writeFileSync, mkdirSync } from "node:fs";
import { deflateSync } from "node:zlib";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SIZE = 1024;
const SS = 4; // supersampling factor per axis
const C = SIZE / 2;

// --- shape tests (all in 1024px canvas coordinates) -------------------------------------

// Apple-like squircle body: 824px wide, centred, superellipse exponent 5.
const BODY_HALF = 412;
const BODY_N = 5;
function inBody(x, y) {
  const a = Math.abs(x - C) / BODY_HALF;
  const b = Math.abs(y - C) / BODY_HALF;
  if (a > 1 || b > 1) return false;
  return Math.pow(a, BODY_N) + Math.pow(b, BODY_N) <= 1;
}

// Signed distance to a rounded box (negative inside).
function sdRoundBox(px, py, cx, cy, half, r) {
  const qx = Math.abs(px - cx) - (half - r);
  const qy = Math.abs(py - cy) - (half - r);
  const ox = Math.max(qx, 0);
  const oy = Math.max(qy, 0);
  return Math.hypot(ox, oy) + Math.min(Math.max(qx, qy), 0) - r;
}

const BLOCK_HALF = 100;
const BLOCK_R = 38;
const BLOCKS = [
  { cx: 392, cy: 632, alpha: 1.0 }, // bottom-left, the "base" block
  { cx: 632, cy: 632, alpha: 0.82 }, // bottom-right
  { cx: 392, cy: 392, alpha: 0.82 }, // top-left
];

// Four-point spark: |x/r|^p + |y/r|^p <= 1 with p < 1 gives concave sides.
const SPARK = { cx: 640, cy: 384, r: 175, p: 0.62 };
function inSpark(x, y) {
  const a = Math.abs(x - SPARK.cx) / SPARK.r;
  const b = Math.abs(y - SPARK.cy) / SPARK.r;
  if (a > 1 || b > 1) return false;
  return Math.pow(a, SPARK.p) + Math.pow(b, SPARK.p) <= 1;
}

// --- colour -------------------------------------------------------------------------------

const hex = (h) => [(h >> 16) & 255, (h >> 8) & 255, h & 255];
const TOP = hex(0x4f46e5); // indigo
const BOTTOM = hex(0x0ea5e9); // sky blue
const WHITE = [255, 255, 255];
const GOLD = hex(0xffd166);

const mix = (a, b, t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
const over = (dst, src, a) => mix(dst, src, a); // dst is opaque inside the body

function shade(x, y) {
  // Body gradient along the main diagonal.
  const t = Math.min(1, Math.max(0, ((x - 100) + (y - 100)) / (2 * 824)));
  let col = mix(TOP, BOTTOM, t);
  // Soft top sheen.
  const sheen = Math.max(0, 1 - (y - 100) / 420) * 0.14;
  col = over(col, WHITE, sheen);
  // Blocks.
  for (const b of BLOCKS) {
    if (sdRoundBox(x, y, b.cx, b.cy, BLOCK_HALF, BLOCK_R) <= 0) col = over(col, WHITE, b.alpha);
  }
  // Spark on top.
  if (inSpark(x, y)) col = GOLD;
  return col;
}

// --- rasteriser -----------------------------------------------------------------------------

const rgba = Buffer.alloc(SIZE * SIZE * 4);
const sub = 1 / SS;
for (let py = 0; py < SIZE; py++) {
  for (let px = 0; px < SIZE; px++) {
    // Cheap reject for the transparent margin.
    if (px < 96 || px > 928 || py < 96 || py > 928) continue;
    let r = 0, g = 0, b = 0, cover = 0;
    for (let sy = 0; sy < SS; sy++) {
      for (let sx = 0; sx < SS; sx++) {
        const x = px + (sx + 0.5) * sub;
        const y = py + (sy + 0.5) * sub;
        if (!inBody(x, y)) continue;
        const c = shade(x, y);
        r += c[0]; g += c[1]; b += c[2]; cover++;
      }
    }
    if (cover === 0) continue;
    const o = (py * SIZE + px) * 4;
    rgba[o] = Math.round(r / cover);
    rgba[o + 1] = Math.round(g / cover);
    rgba[o + 2] = Math.round(b / cover);
    rgba[o + 3] = Math.round((cover / (SS * SS)) * 255);
  }
}

// --- PNG encoder ------------------------------------------------------------------------------

const crcTable = new Uint32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
function crc32(buf) {
  let c = 0xffffffff;
  for (const byte of buf) c = crcTable[(c ^ byte) & 255] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // RGBA
const raw = Buffer.alloc((SIZE * 4 + 1) * SIZE);
for (let y = 0; y < SIZE; y++) {
  raw[y * (SIZE * 4 + 1)] = 0; // filter: none
  rgba.copy(raw, y * (SIZE * 4 + 1) + 1, y * SIZE * 4, (y + 1) * SIZE * 4);
}
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

const here = dirname(fileURLToPath(import.meta.url));
const out = resolve(here, "..", "src-tauri", "icons", "source-icon.png");
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, png);
console.log(`wrote ${out} (${png.length} bytes)`);
