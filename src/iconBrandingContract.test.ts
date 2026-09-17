import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const iconDirectory = resolve(repositoryRoot, "src-tauri/icons");
const sourcePath = resolve(iconDirectory, "icon-source.svg");
const contractPath = resolve(iconDirectory, "icon-contract.json");
const generatorPath = resolve(repositoryRoot, "scripts/generate-icons.mjs");
const generatedDirectory = mkdtempSync(join(tmpdir(), "disksage-icons-"));

const EXPECTED_SOURCE_SHA256 =
  "42ba6bb93f6ee9faf4783adc279dee900fd2a33c14a926a7bbb16685c0862419";

interface PngImage {
  height: number;
  rgba: Buffer;
  width: number;
}

interface PngAssetContract {
  height: number;
  mode: "RGBA";
  path: string;
  width: number;
}

interface IconContract {
  brand: string;
  generator: string;
  generator_sha256: string;
  icns: {
    chunks: Array<{ size: number; type: string }>;
    path: string;
  };
  ico: {
    bit_depth: number;
    path: string;
    sizes: number[];
  };
  png_assets: PngAssetContract[];
  schema: string;
  source: string;
  source_sha256: string;
}

interface GeneratedAsset {
  bytes?: number;
  height?: number;
  mode?: string;
  path: string;
  sha256: string;
  width?: number;
}

interface GeneratedManifest {
  assets: GeneratedAsset[];
  brand: string;
  generator: string;
  generator_sha256: string;
  schema: string;
  source: string;
  source_sha256: string;
}

function sha256(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

function readJson<T>(path: string): T {
  return JSON.parse(readFileSync(path, "utf8")) as T;
}

function readPng(path: string): PngImage {
  const bytes = readFileSync(path);
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  expect(bytes.subarray(0, 8)).toEqual(signature);

  let offset = 8;
  let width = 0;
  let height = 0;
  const compressedRows: Buffer[] = [];
  while (offset < bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const type = bytes.toString("ascii", offset + 4, offset + 8);
    const payload = bytes.subarray(offset + 8, offset + 8 + length);
    if (type === "IHDR") {
      width = payload.readUInt32BE(0);
      height = payload.readUInt32BE(4);
      expect(payload[8]).toBe(8);
      expect(payload[9]).toBe(6);
    } else if (type === "IDAT") {
      compressedRows.push(payload);
    }
    offset += length + 12;
  }

  const inflated = inflateSync(Buffer.concat(compressedRows));
  const rowLength = width * 4;
  const rgba = Buffer.alloc(rowLength * height);
  for (let row = 0; row < height; row += 1) {
    const inflatedOffset = row * (rowLength + 1);
    expect(inflated[inflatedOffset]).toBe(0);
    inflated.copy(
      rgba,
      row * rowLength,
      inflatedOffset + 1,
      inflatedOffset + 1 + rowLength,
    );
  }
  return { height, rgba, width };
}

function pixelAt(image: PngImage, x: number, y: number): number[] {
  const offset = (y * image.width + x) * 4;
  return [...image.rgba.subarray(offset, offset + 4)];
}

function runGenerator(
  source: string,
  contract: IconContract,
): { generatedPath: string; rootPath: string } {
  const rootPath = mkdtempSync(join(tmpdir(), "disksage-icon-variant-"));
  const variantSourcePath = resolve(rootPath, "icon-source.svg");
  const variantContractPath = resolve(rootPath, "icon-contract.json");
  const generatedPath = resolve(rootPath, "generated");

  writeFileSync(variantSourcePath, source);
  writeFileSync(variantContractPath, `${JSON.stringify(contract, null, 2)}\n`);
  execFileSync(
    process.execPath,
    [
      generatorPath,
      "--source",
      variantSourcePath,
      "--contract",
      variantContractPath,
      "--output",
      generatedPath,
    ],
    { stdio: "pipe" },
  );
  return { generatedPath, rootPath };
}

beforeAll(() => {
  execFileSync(
    process.execPath,
    [
      generatorPath,
      "--source",
      sourcePath,
      "--contract",
      contractPath,
      "--output",
      generatedDirectory,
    ],
    { stdio: "pipe" },
  );
});

afterAll(() => {
  rmSync(generatedDirectory, { force: true, recursive: true });
});

describe("DiskSage product icon identity", () => {
  it("uses one reviewable vector source for the desktop and favicon identity", () => {
    const source = readFileSync(sourcePath, "utf8");
    const favicon = readFileSync(resolve(repositoryRoot, "static/favicon.svg"), "utf8");

    expect(source).toBe(favicon);
    expect(sha256(Buffer.from(source))).toBe(EXPECTED_SOURCE_SHA256);
    expect(source).toContain('data-brand="disksage-v1"');
    expect(source).toContain("<title>DiskSage</title>");
    expect(source).toContain("#12314A");
    expect(source).toContain("#2F9E74");
    expect(source).toContain("#F2B134");
    expect(source).not.toMatch(/(?:Svelte|Tauri)/i);
  });

  it("uses the canonical SVG geometry and palette as native raster input", () => {
    const originalSource = readFileSync(sourcePath, "utf8");
    const originalContract = readJson<IconContract>(contractPath);
    const variantSource = originalSource.replace("#F2B134", "#CC00FF");
    const variantContract = {
      ...originalContract,
      source_sha256: sha256(Buffer.from(variantSource)),
    };
    const { generatedPath, rootPath } = runGenerator(variantSource, variantContract);

    try {
      const image = readPng(resolve(generatedPath, "icon.png"));
      expect(pixelAt(image, 395, 139)).toEqual([204, 0, 255, 255]);
    } finally {
      rmSync(rootPath, { force: true, recursive: true });
    }
  });

  it("renders the intended disk, verified-action, and insight colors", () => {
    const image = readPng(resolve(generatedDirectory, "icon.png"));

    expect([image.width, image.height]).toEqual([512, 512]);
    expect(pixelAt(image, 0, 0)).toEqual([0, 0, 0, 0]);
    expect(pixelAt(image, 64, 256)).toEqual([18, 49, 74, 255]);
    expect(pixelAt(image, 256, 125)).toEqual([244, 247, 249, 255]);
    expect(pixelAt(image, 256, 190)).toEqual([220, 232, 238, 255]);
    expect(pixelAt(image, 256, 256)).toEqual([18, 49, 74, 255]);
    expect(pixelAt(image, 275, 283)).toEqual([47, 158, 116, 255]);
    expect(pixelAt(image, 395, 139)).toEqual([242, 177, 52, 255]);
  });

  it("generates every contracted square RGBA PNG and records its digest", () => {
    const contract = readJson<IconContract>(contractPath);
    const manifest = readJson<GeneratedManifest>(
      resolve(generatedDirectory, "icon-manifest.json"),
    );

    expect(contract).toMatchObject({
      brand: "DiskSage",
      generator: "scripts/generate-icons.mjs",
      generator_sha256: sha256(readFileSync(generatorPath)),
      schema: "disksage.icon-contract/v1",
      source: "icon-source.svg",
      source_sha256: EXPECTED_SOURCE_SHA256,
    });
    expect(manifest).toMatchObject({
      brand: contract.brand,
      generator: contract.generator,
      generator_sha256: contract.generator_sha256,
      schema: "disksage.icon-set/v1",
      source: contract.source,
      source_sha256: contract.source_sha256,
    });

    for (const expected of contract.png_assets) {
      const imagePath = resolve(generatedDirectory, expected.path);
      const image = readPng(imagePath);
      const recorded = manifest.assets.find(({ path }) => path === expected.path);

      expect(expected.width).toBe(expected.height);
      expect(expected.mode).toBe("RGBA");
      expect([image.width, image.height]).toEqual([expected.width, expected.height]);
      expect(recorded).toMatchObject(expected);
      expect(recorded?.sha256).toBe(sha256(readFileSync(imagePath)));
    }
  });

  it("generates the contracted Windows ICO layer order and bit depth", () => {
    const contract = readJson<IconContract>(contractPath);
    const bytes = readFileSync(resolve(generatedDirectory, contract.ico.path));
    const manifest = readJson<GeneratedManifest>(
      resolve(generatedDirectory, "icon-manifest.json"),
    );

    expect(bytes.readUInt16LE(0)).toBe(0);
    expect(bytes.readUInt16LE(2)).toBe(1);
    expect(bytes.readUInt16LE(4)).toBe(contract.ico.sizes.length);

    const sizes: number[] = [];
    const bitDepths: number[] = [];
    for (let index = 0; index < contract.ico.sizes.length; index += 1) {
      const offset = 6 + index * 16;
      const width = bytes[offset] === 0 ? 256 : bytes[offset];
      const height = bytes[offset + 1] === 0 ? 256 : bytes[offset + 1];
      sizes.push(width);
      expect(height).toBe(width);
      bitDepths.push(bytes.readUInt16LE(offset + 6));
    }

    expect(sizes).toEqual(contract.ico.sizes);
    expect(bitDepths).toEqual(contract.ico.sizes.map(() => contract.ico.bit_depth));
    expect(manifest.assets.find(({ path }) => path === contract.ico.path)?.sha256).toBe(
      sha256(bytes),
    );
  });

  it("generates every contracted modern macOS ICNS chunk", () => {
    const contract = readJson<IconContract>(contractPath);
    const bytes = readFileSync(resolve(generatedDirectory, contract.icns.path));
    const manifest = readJson<GeneratedManifest>(
      resolve(generatedDirectory, "icon-manifest.json"),
    );

    expect(bytes.subarray(0, 4).toString("ascii")).toBe("icns");
    expect(bytes.readUInt32BE(4)).toBe(bytes.length);
    for (const { type } of contract.icns.chunks) {
      expect(bytes.subarray(8).includes(Buffer.from(type))).toBe(true);
    }
    expect(manifest.assets.find(({ path }) => path === contract.icns.path)?.sha256).toBe(
      sha256(bytes),
    );
  });

  it("keeps canonical Tauri commands while npm lifecycle hooks generate icon resources", () => {
    const config = readJson<{
      build: { beforeBuildCommand: string; beforeDevCommand: string };
      bundle: { resources: string[] };
    }>(resolve(repositoryRoot, "src-tauri/tauri.conf.json"));
    const packageJson = readJson<{
      scripts: { prebuild?: string; predev?: string };
    }>(resolve(repositoryRoot, "package.json"));

    expect(config.build.beforeDevCommand).toBe("npm run dev");
    expect(config.build.beforeBuildCommand).toBe("npm run build");
    expect(packageJson.scripts.predev).toBe("node scripts/generate-icons.mjs");
    expect(packageJson.scripts.prebuild).toBe("node scripts/generate-icons.mjs");
    expect(config.bundle.resources).toContain("icons/icon-manifest.json");
  });
});
