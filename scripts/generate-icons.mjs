#!/usr/bin/env node

import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";

const MODULE_DIRECTORY = dirname(fileURLToPath(import.meta.url));
const REPOSITORY_ROOT = resolve(MODULE_DIRECTORY, "..");

const DESIGN_SIZE = 1024;
const MASTER_SIZE = 2048;
const SAMPLE_SCALE = 4;

const COLORS = Object.freeze({
  transparent: [0, 0, 0, 0],
  navy: [0x12, 0x31, 0x4a, 0xff],
  platter: [0xf4, 0xf7, 0xf9, 0xff],
  ring: [0xdc, 0xe8, 0xee, 0xff],
  green: [0x2f, 0x9e, 0x74, 0xff],
  gold: [0xf2, 0xb1, 0x34, 0xff],
});

const STAR_POINTS = Object.freeze([
  [790, 182],
  [818, 250],
  [886, 278],
  [818, 306],
  [790, 374],
  [762, 306],
  [694, 278],
  [762, 250],
]);

/** Return a SHA-256 digest for a generated asset. */
function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

/** Return whether a normalized point falls inside the rounded product tile. */
function insideRoundedTile(x, y) {
  if (x < 64 || x > 960 || y < 64 || y > 960) return false;
  const dx = Math.max(288 - x, 0, x - 736);
  const dy = Math.max(288 - y, 0, y - 736);
  return dx * dx + dy * dy <= 224 * 224;
}

/** Return squared distance from a point to a finite line segment. */
function squaredDistanceToSegment(px, py, ax, ay, bx, by) {
  const abx = bx - ax;
  const aby = by - ay;
  const denominator = abx * abx + aby * aby;
  const projection = denominator === 0
    ? 0
    : Math.max(0, Math.min(1, ((px - ax) * abx + (py - ay) * aby) / denominator));
  const dx = px - (ax + projection * abx);
  const dy = py - (ay + projection * aby);
  return dx * dx + dy * dy;
}

/** Return whether a point falls inside the eight-point insight sparkle. */
function insidePolygon(x, y, points) {
  let inside = false;
  for (let current = 0, previous = points.length - 1; current < points.length; previous = current++) {
    const [currentX, currentY] = points[current];
    const [previousX, previousY] = points[previous];
    const crosses = (currentY > y) !== (previousY > y)
      && x < ((previousX - currentX) * (y - currentY)) / (previousY - currentY) + currentX;
    if (crosses) inside = !inside;
  }
  return inside;
}

/** Resolve the topmost opaque brand color at one normalized design-space point. */
function colorAt(x, y) {
  let color = COLORS.transparent;

  if (insideRoundedTile(x, y)) color = COLORS.navy;

  const centerX = x - 512;
  const centerY = y - 512;
  const radiusSquared = centerX * centerX + centerY * centerY;
  if (radiusSquared <= 300 * 300) color = COLORS.platter;
  if (radiusSquared <= 166 * 166) color = COLORS.ring;
  if (radiusSquared <= 68 * 68) color = COLORS.navy;

  const checkRadiusSquared = 39 * 39;
  if (
    squaredDistanceToSegment(x, y, 320, 525, 458, 663) <= checkRadiusSquared
    || squaredDistanceToSegment(x, y, 458, 663, 720, 388) <= checkRadiusSquared
  ) {
    color = COLORS.green;
  }

  if (insidePolygon(x, y, STAR_POINTS)) color = COLORS.gold;
  return color;
}

/** Rasterize the brand directly at a sample-grid resolution. */
function rasterizeSamples(size) {
  const pixels = new Uint8Array(size * size * 4);
  const designPerPixel = DESIGN_SIZE / size;
  let offset = 0;

  for (let y = 0; y < size; y += 1) {
    const normalizedY = (y + 0.5) * designPerPixel;
    for (let x = 0; x < size; x += 1) {
      const normalizedX = (x + 0.5) * designPerPixel;
      const [red, green, blue, alpha] = colorAt(normalizedX, normalizedY);
      pixels[offset] = red;
      pixels[offset + 1] = green;
      pixels[offset + 2] = blue;
      pixels[offset + 3] = alpha;
      offset += 4;
    }
  }

  return pixels;
}

/** Downsample an integer-multiple RGBA sample grid with alpha-correct box averaging. */
function downsample(source, sourceSize, targetSize) {
  if (sourceSize % targetSize !== 0) {
    throw new Error(`source size ${sourceSize} is not divisible by target size ${targetSize}`);
  }

  const factor = sourceSize / targetSize;
  const sampleCount = factor * factor;
  const output = new Uint8Array(targetSize * targetSize * 4);

  for (let targetY = 0; targetY < targetSize; targetY += 1) {
    for (let targetX = 0; targetX < targetSize; targetX += 1) {
      let alphaSum = 0;
      let redPremultiplied = 0;
      let greenPremultiplied = 0;
      let bluePremultiplied = 0;

      for (let sampleY = 0; sampleY < factor; sampleY += 1) {
        const sourceY = targetY * factor + sampleY;
        for (let sampleX = 0; sampleX < factor; sampleX += 1) {
          const sourceX = targetX * factor + sampleX;
          const sourceOffset = (sourceY * sourceSize + sourceX) * 4;
          const alpha = source[sourceOffset + 3];
          alphaSum += alpha;
          redPremultiplied += source[sourceOffset] * alpha;
          greenPremultiplied += source[sourceOffset + 1] * alpha;
          bluePremultiplied += source[sourceOffset + 2] * alpha;
        }
      }

      const outputOffset = (targetY * targetSize + targetX) * 4;
      output[outputOffset + 3] = Math.round(alphaSum / sampleCount);
      if (alphaSum > 0) {
        output[outputOffset] = Math.round(redPremultiplied / alphaSum);
        output[outputOffset + 1] = Math.round(greenPremultiplied / alphaSum);
        output[outputOffset + 2] = Math.round(bluePremultiplied / alphaSum);
      }
    }
  }

  return output;
}

/** Generate one antialiased RGBA icon, reusing a high-resolution master where possible. */
function renderIcon(size, master) {
  if (MASTER_SIZE % size === 0) return downsample(master, MASTER_SIZE, size);
  const samples = rasterizeSamples(size * SAMPLE_SCALE);
  return downsample(samples, size * SAMPLE_SCALE, size);
}

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let value = 0; value < 256; value += 1) {
    let checksum = value;
    for (let bit = 0; bit < 8; bit += 1) {
      checksum = checksum & 1 ? 0xedb88320 ^ (checksum >>> 1) : checksum >>> 1;
    }
    table[value] = checksum >>> 0;
  }
  return table;
})();

/** Compute the PNG CRC-32 checksum for a chunk type and payload. */
function crc32(bytes) {
  let checksum = 0xffffffff;
  for (const byte of bytes) checksum = CRC_TABLE[(checksum ^ byte) & 0xff] ^ (checksum >>> 8);
  return (checksum ^ 0xffffffff) >>> 0;
}

/** Build one length-prefixed PNG chunk. */
function pngChunk(type, data) {
  const typeBytes = Buffer.from(type, "ascii");
  const output = Buffer.allocUnsafe(12 + data.length);
  output.writeUInt32BE(data.length, 0);
  typeBytes.copy(output, 4);
  data.copy(output, 8);
  output.writeUInt32BE(crc32(Buffer.concat([typeBytes, data])), 8 + data.length);
  return output;
}

/** Encode an 8-bit RGBA raster as a deterministic, metadata-free PNG. */
function encodePng(width, height, rgba) {
  const rowLength = width * 4;
  const raw = Buffer.allocUnsafe((rowLength + 1) * height);
  for (let y = 0; y < height; y += 1) {
    const rawOffset = y * (rowLength + 1);
    raw[rawOffset] = 0;
    Buffer.from(rgba.buffer, rgba.byteOffset + y * rowLength, rowLength).copy(raw, rawOffset + 1);
  }

  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = 6;
  header[10] = 0;
  header[11] = 0;
  header[12] = 0;

  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk("IHDR", header),
    pngChunk("IDAT", deflateSync(raw, { level: 9 })),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

/** Assemble a Windows ICO whose image entries are PNG payloads in the required order. */
function encodeIco(pngBySize, sizes, bitDepth) {
  const header = Buffer.alloc(6 + sizes.length * 16);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(sizes.length, 4);

  let payloadOffset = header.length;
  const payloads = [];
  sizes.forEach((size, index) => {
    const png = pngBySize.get(size);
    if (!png) throw new Error(`missing ${size}px PNG for ICO`);
    const entryOffset = 6 + index * 16;
    header[entryOffset] = size === 256 ? 0 : size;
    header[entryOffset + 1] = size === 256 ? 0 : size;
    header[entryOffset + 2] = 0;
    header[entryOffset + 3] = 0;
    header.writeUInt16LE(1, entryOffset + 4);
    header.writeUInt16LE(bitDepth, entryOffset + 6);
    header.writeUInt32LE(png.length, entryOffset + 8);
    header.writeUInt32LE(payloadOffset, entryOffset + 12);
    payloadOffset += png.length;
    payloads.push(png);
  });

  return Buffer.concat([header, ...payloads]);
}

/** Assemble a macOS ICNS container from PNG-backed modern icon chunks. */
function encodeIcns(pngBySize, chunksContract) {
  const chunks = chunksContract.map(({ type, size }) => {
    const png = pngBySize.get(size);
    if (!png) throw new Error(`missing ${size}px PNG for ICNS`);
    const chunk = Buffer.allocUnsafe(8 + png.length);
    chunk.write(type, 0, 4, "ascii");
    chunk.writeUInt32BE(chunk.length, 4);
    png.copy(chunk, 8);
    return chunk;
  });
  const length = 8 + chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const header = Buffer.alloc(8);
  header.write("icns", 0, 4, "ascii");
  header.writeUInt32BE(length, 4);
  return Buffer.concat([header, ...chunks]);
}

/** Generate every tracked desktop/store icon and an integrity manifest. */
export function generateIconSet({ sourcePath, contractPath, outputDirectory }) {
  const source = readFileSync(sourcePath);
  const sourceDigest = sha256(source);
  const contract = JSON.parse(readFileSync(contractPath, "utf8"));
  if (contract.source_sha256 !== sourceDigest) {
    throw new Error(
      `icon source digest ${sourceDigest} does not match contract ${contract.source_sha256}`,
    );
  }
  const master = rasterizeSamples(MASTER_SIZE);
  const requiredSizes = new Set([
    ...contract.png_assets.map(({ width }) => width),
    ...contract.ico.sizes,
    ...contract.icns.chunks.map(({ size }) => size),
  ]);
  const pngBySize = new Map();

  for (const size of [...requiredSizes].sort((left, right) => left - right)) {
    const rgba = renderIcon(size, master);
    pngBySize.set(size, encodePng(size, size, rgba));
  }

  mkdirSync(outputDirectory, { recursive: true });
  const assets = [];
  for (const asset of contract.png_assets) {
    if (asset.width !== asset.height || asset.mode !== "RGBA") {
      throw new Error(`unsupported PNG contract for ${asset.path}`);
    }
    const png = pngBySize.get(asset.width);
    writeFileSync(resolve(outputDirectory, asset.path), png);
    assets.push({ ...asset, sha256: sha256(png) });
  }

  const ico = encodeIco(pngBySize, contract.ico.sizes, contract.ico.bit_depth);
  writeFileSync(resolve(outputDirectory, contract.ico.path), ico);
  assets.push({ path: contract.ico.path, sha256: sha256(ico), bytes: ico.length });

  const icns = encodeIcns(pngBySize, contract.icns.chunks);
  writeFileSync(resolve(outputDirectory, contract.icns.path), icns);
  assets.push({ path: contract.icns.path, sha256: sha256(icns), bytes: icns.length });

  const manifest = {
    schema: "disksage.icon-set/v1",
    brand: contract.brand,
    source: contract.source,
    source_sha256: sourceDigest,
    generator: contract.generator,
    assets,
  };
  writeFileSync(
    resolve(outputDirectory, "icon-manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );
  return manifest;
}

/** Parse the tiny CLI surface without introducing a runtime dependency. */
function parseArguments(argumentsList) {
  let sourcePath = resolve(REPOSITORY_ROOT, "src-tauri/icons/icon-source.svg");
  let contractPath = resolve(REPOSITORY_ROOT, "src-tauri/icons/icon-contract.json");
  let outputDirectory = resolve(REPOSITORY_ROOT, "src-tauri/icons");
  for (let index = 0; index < argumentsList.length; index += 1) {
    const argument = argumentsList[index];
    if (argument === "--source") sourcePath = resolve(argumentsList[++index]);
    else if (argument === "--contract") contractPath = resolve(argumentsList[++index]);
    else if (argument === "--output") outputDirectory = resolve(argumentsList[++index]);
    else throw new Error(`unknown icon-generator argument: ${argument}`);
  }
  return { sourcePath, contractPath, outputDirectory };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  generateIconSet(parseArguments(process.argv.slice(2)));
}
