#!/usr/bin/env node

import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";

const MODULE_DIRECTORY = dirname(fileURLToPath(import.meta.url));
const REPOSITORY_ROOT = resolve(MODULE_DIRECTORY, "..");

const MASTER_SIZE = 2048;
const SAMPLE_SCALE = 4;
const TRANSPARENT = Object.freeze([0, 0, 0, 0]);

/** Return a SHA-256 digest for a source or generated asset. */
function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

/** Fail closed unless the invoked generator bytes match the reviewed contract. */
function validateGeneratorIdentity(contract) {
  const generatorPath = fileURLToPath(import.meta.url);
  const generatorDigest = sha256(readFileSync(generatorPath));
  if (
    contract.generator !== "scripts/generate-icons.mjs"
    || contract.generator_sha256 !== generatorDigest
  ) {
    throw new Error("icon-generator-identity-mismatch");
  }
  return generatorDigest;
}

/** Fail closed if the compressor runtime differs from the audited generation contract. */
function validateGeneratorRuntime(contract) {
  const expected = contract.generator_runtime;
  const nodeMajor = Number.parseInt(process.versions.node.split(".")[0] ?? "", 10);
  if (
    !expected
    || expected.node_major !== nodeMajor
    || expected.zlib !== process.versions.zlib
  ) {
    throw new Error("icon-generator-runtime-mismatch");
  }
}

/** Parse one element's double-quoted SVG attributes into a plain object. */
function parseAttributes(fragment) {
  const attributes = {};
  for (const match of fragment.matchAll(/([A-Za-z_:][\w:.-]*)="([^"]*)"/g)) {
    attributes[match[1]] = match[2];
  }
  return attributes;
}

/** Parse one required finite numeric SVG attribute. */
function numericAttribute(attributes, name, elementName) {
  const value = Number(attributes[name]);
  if (!Number.isFinite(value)) {
    throw new Error(`${elementName} requires finite numeric attribute ${name}`);
  }
  return value;
}

/** Parse a #RRGGBB SVG color into opaque RGBA bytes. */
function parseColor(value, elementName) {
  const match = /^#([0-9a-fA-F]{6})$/.exec(value ?? "");
  if (!match) {
    throw new Error(`${elementName} requires an explicit #RRGGBB color`);
  }
  const numeric = Number.parseInt(match[1], 16);
  return [
    (numeric >>> 16) & 0xff,
    (numeric >>> 8) & 0xff,
    numeric & 0xff,
    0xff,
  ];
}

/** Parse coordinate pairs from the deliberately tiny path subset used by the product mark. */
function parsePathPoints(pathData, elementName) {
  const values = pathData.match(/-?(?:\d+(?:\.\d*)?|\.\d+)/g)?.map(Number) ?? [];
  if (values.length < 4 || values.length % 2 !== 0 || values.some((value) => !Number.isFinite(value))) {
    throw new Error(`${elementName} requires at least two finite coordinate pairs`);
  }
  const points = [];
  for (let index = 0; index < values.length; index += 2) {
    points.push([values[index], values[index + 1]]);
  }
  return points;
}

/**
 * Parse the canonical icon SVG into the restricted geometry that the deterministic
 * rasterizer supports. Unsupported or ambiguous shapes fail closed.
 */
function parseIconSource(sourceBytes) {
  const source = Buffer.from(sourceBytes).toString("utf8");
  const svgMatch = /<svg\b([^>]*)>/i.exec(source);
  if (!svgMatch) throw new Error("icon source requires an <svg> root");
  const svgAttributes = parseAttributes(svgMatch[1]);
  const viewBox = (svgAttributes.viewBox ?? "").trim().split(/\s+/).map(Number);
  if (
    viewBox.length !== 4
    || viewBox.some((value) => !Number.isFinite(value))
    || viewBox[0] !== 0
    || viewBox[1] !== 0
    || viewBox[2] <= 0
    || viewBox[2] !== viewBox[3]
  ) {
    throw new Error("icon source requires a positive square viewBox starting at 0 0");
  }

  const shapes = [];
  const shapePattern = /<(rect|circle|path)\b([^>]*)\/?>/gi;
  for (const match of source.matchAll(shapePattern)) {
    const type = match[1].toLowerCase();
    const attributes = parseAttributes(match[2]);
    if (type === "rect") {
      const rx = attributes.rx === undefined ? 0 : numericAttribute(attributes, "rx", "rect");
      const ry = attributes.ry === undefined ? rx : numericAttribute(attributes, "ry", "rect");
      if (rx !== ry || rx < 0) {
        throw new Error("icon rect requires a non-negative equal rx/ry radius");
      }
      shapes.push({
        type: "rounded_rect",
        x: numericAttribute(attributes, "x", "rect"),
        y: numericAttribute(attributes, "y", "rect"),
        width: numericAttribute(attributes, "width", "rect"),
        height: numericAttribute(attributes, "height", "rect"),
        radius: rx,
        color: parseColor(attributes.fill, "rect"),
      });
    } else if (type === "circle") {
      shapes.push({
        type: "circle",
        cx: numericAttribute(attributes, "cx", "circle"),
        cy: numericAttribute(attributes, "cy", "circle"),
        radius: numericAttribute(attributes, "r", "circle"),
        color: parseColor(attributes.fill, "circle"),
      });
    } else {
      const points = parsePathPoints(attributes.d ?? "", "path");
      const fill = attributes.fill ?? "";
      const stroke = attributes.stroke ?? "";
      if (fill !== "" && fill.toLowerCase() !== "none") {
        if (!/[zZ]\s*$/.test(attributes.d ?? "")) {
          throw new Error("filled icon paths must be explicitly closed");
        }
        shapes.push({
          type: "polygon",
          points,
          color: parseColor(fill, "filled path"),
        });
      } else if (stroke !== "") {
        const strokeWidth = numericAttribute(attributes, "stroke-width", "stroked path");
        if (
          strokeWidth <= 0
          || (attributes["stroke-linecap"] ?? "round") !== "round"
          || (attributes["stroke-linejoin"] ?? "round") !== "round"
        ) {
          throw new Error("stroked icon paths require positive width with round caps and joins");
        }
        shapes.push({
          type: "polyline",
          points,
          radius: strokeWidth / 2,
          color: parseColor(stroke, "stroked path"),
        });
      } else {
        throw new Error("icon path requires either a fill or a stroke");
      }
    }
  }

  if (shapes.length === 0) throw new Error("icon source contains no supported geometry");
  return { designSize: viewBox[2], shapes };
}

/** Return whether a point falls inside one rounded rectangle. */
function insideRoundedRectangle(px, py, shape) {
  const right = shape.x + shape.width;
  const bottom = shape.y + shape.height;
  if (px < shape.x || px > right || py < shape.y || py > bottom) return false;
  if (shape.radius === 0) return true;
  const centerX = Math.max(shape.x + shape.radius, Math.min(px, right - shape.radius));
  const centerY = Math.max(shape.y + shape.radius, Math.min(py, bottom - shape.radius));
  const dx = px - centerX;
  const dy = py - centerY;
  return dx * dx + dy * dy <= shape.radius * shape.radius;
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

/** Return whether a point falls inside one polygon. */
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

/** Return whether a point falls inside the opaque coverage of one parsed shape. */
function shapeContains(shape, x, y) {
  if (shape.type === "rounded_rect") return insideRoundedRectangle(x, y, shape);
  if (shape.type === "circle") {
    const dx = x - shape.cx;
    const dy = y - shape.cy;
    return dx * dx + dy * dy <= shape.radius * shape.radius;
  }
  if (shape.type === "polygon") return insidePolygon(x, y, shape.points);
  if (shape.type === "polyline") {
    const radiusSquared = shape.radius * shape.radius;
    for (let index = 1; index < shape.points.length; index += 1) {
      const [ax, ay] = shape.points[index - 1];
      const [bx, by] = shape.points[index];
      if (squaredDistanceToSegment(x, y, ax, ay, bx, by) <= radiusSquared) return true;
    }
    return false;
  }
  throw new Error(`unsupported parsed icon shape: ${shape.type}`);
}

/** Resolve the topmost opaque brand color at one design-space point. */
function colorAt(sourceDefinition, x, y) {
  let color = TRANSPARENT;
  for (const shape of sourceDefinition.shapes) {
    if (shapeContains(shape, x, y)) color = shape.color;
  }
  return color;
}

/** Rasterize the parsed canonical SVG directly at a sample-grid resolution. */
function rasterizeSamples(size, sourceDefinition) {
  const pixels = new Uint8Array(size * size * 4);
  const designPerPixel = sourceDefinition.designSize / size;
  let offset = 0;

  for (let y = 0; y < size; y += 1) {
    const normalizedY = (y + 0.5) * designPerPixel;
    for (let x = 0; x < size; x += 1) {
      const normalizedX = (x + 0.5) * designPerPixel;
      const [red, green, blue, alpha] = colorAt(sourceDefinition, normalizedX, normalizedY);
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
function renderIcon(size, master, sourceDefinition) {
  if (MASTER_SIZE % size === 0) return downsample(master, MASTER_SIZE, size);
  const samples = rasterizeSamples(size * SAMPLE_SCALE, sourceDefinition);
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

/** Encode one 32-bit Windows icon DIB with bottom-up BGRA pixels and a DWORD-aligned AND mask. */
function encodeIconDib(size, rgba, bitDepth) {
  if (bitDepth !== 32) throw new Error(`unsupported ICO bit depth: ${bitDepth}`);
  if (rgba.length !== size * size * 4) throw new Error(`invalid ${size}px RGBA raster for ICO`);

  const pixelBytes = size * size * 4;
  const maskRowBytes = Math.ceil(size / 32) * 4;
  const maskBytes = maskRowBytes * size;
  const header = Buffer.alloc(40);
  header.writeUInt32LE(40, 0);
  header.writeInt32LE(size, 4);
  header.writeInt32LE(size * 2, 8);
  header.writeUInt16LE(1, 12);
  header.writeUInt16LE(bitDepth, 14);
  header.writeUInt32LE(0, 16);
  header.writeUInt32LE(pixelBytes + maskBytes, 20);

  const pixels = Buffer.allocUnsafe(pixelBytes);
  const mask = Buffer.alloc(maskBytes);
  for (let dibY = 0; dibY < size; dibY += 1) {
    const sourceY = size - 1 - dibY;
    for (let x = 0; x < size; x += 1) {
      const sourceOffset = (sourceY * size + x) * 4;
      const destinationOffset = (dibY * size + x) * 4;
      const red = rgba[sourceOffset];
      const green = rgba[sourceOffset + 1];
      const blue = rgba[sourceOffset + 2];
      const alpha = rgba[sourceOffset + 3];
      pixels[destinationOffset] = blue;
      pixels[destinationOffset + 1] = green;
      pixels[destinationOffset + 2] = red;
      pixels[destinationOffset + 3] = alpha;
      if (alpha === 0) {
        const maskOffset = dibY * maskRowBytes + Math.floor(x / 8);
        mask[maskOffset] |= 1 << (7 - (x % 8));
      }
    }
  }

  return Buffer.concat([header, pixels, mask]);
}

/** Assemble a Windows ICO using DIB for legacy-sized layers and PNG for the 256px layer. */
function encodeIco(rgbaBySize, pngBySize, sizes, bitDepth) {
  const header = Buffer.alloc(6 + sizes.length * 16);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(sizes.length, 4);

  let payloadOffset = header.length;
  const payloads = [];
  sizes.forEach((size, index) => {
    const png = pngBySize.get(size);
    const rgba = rgbaBySize.get(size);
    if (!png || !rgba) throw new Error(`missing ${size}px raster for ICO`);
    const payload = size === 256 ? png : encodeIconDib(size, rgba, bitDepth);
    const entryOffset = 6 + index * 16;
    header[entryOffset] = size === 256 ? 0 : size;
    header[entryOffset + 1] = size === 256 ? 0 : size;
    header[entryOffset + 2] = 0;
    header[entryOffset + 3] = 0;
    header.writeUInt16LE(1, entryOffset + 4);
    header.writeUInt16LE(bitDepth, entryOffset + 6);
    header.writeUInt32LE(payload.length, entryOffset + 8);
    header.writeUInt32LE(payloadOffset, entryOffset + 12);
    payloadOffset += payload.length;
    payloads.push(payload);
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

/** Generate every contracted desktop/store icon and an integrity manifest from the canonical SVG. */
export function generateIconSet({ sourcePath, contractPath, outputDirectory }) {
  const source = readFileSync(sourcePath);
  const sourceDigest = sha256(source);
  const contract = JSON.parse(readFileSync(contractPath, "utf8"));
  const generatorDigest = validateGeneratorIdentity(contract);
  validateGeneratorRuntime(contract);
  if (contract.source_sha256 !== sourceDigest) {
    throw new Error(
      `icon source digest ${sourceDigest} does not match contract ${contract.source_sha256}`,
    );
  }

  const sourceDefinition = parseIconSource(source);
  const master = rasterizeSamples(MASTER_SIZE, sourceDefinition);
  const requiredSizes = new Set([
    ...contract.png_assets.map(({ width }) => width),
    ...contract.ico.sizes,
    ...contract.icns.chunks.map(({ size }) => size),
  ]);
  const rgbaBySize = new Map();
  const pngBySize = new Map();

  for (const size of [...requiredSizes].sort((left, right) => left - right)) {
    const rgba = renderIcon(size, master, sourceDefinition);
    rgbaBySize.set(size, rgba);
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

  const ico = encodeIco(rgbaBySize, pngBySize, contract.ico.sizes, contract.ico.bit_depth);
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
    generator_sha256: generatorDigest,
    generator_runtime: {
      node: process.versions.node,
      zlib: process.versions.zlib,
    },
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
