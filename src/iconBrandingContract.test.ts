import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const iconDirectory = resolve(repositoryRoot, "src-tauri/icons");

interface ExpectedPng {
  sha256: string;
  width: number;
  height: number;
}

const EXPECTED_SOURCE_SHA256 =
  "42ba6bb93f6ee9faf4783adc279dee900fd2a33c14a926a7bbb16685c0862419";

const EXPECTED_PNGS: Readonly<Record<string, ExpectedPng>> = {
  "32x32.png": {
    sha256: "d170f7c9d6a72fc04868ade5c140d0987fb712a928adafe0b5c69cb7c7de28df",
    width: 32,
    height: 32,
  },
  "128x128.png": {
    sha256: "45198d0ff49793f066997958ded2caf9362147c8ffe46ff8d8864d32e2db6f7b",
    width: 128,
    height: 128,
  },
  "128x128-2x.png": {
    sha256: "962e380359437906f7493a1f4b758dda76fc071e99ecd810a1284e4d7b3b4cec",
    width: 256,
    height: 256,
  },
  "Square30x30Logo.png": {
    sha256: "99c69b29c2f9337570297a3a380adb8d88b6b36d3bbef6e3177a5b4ae54cad8b",
    width: 30,
    height: 30,
  },
  "Square44x44Logo.png": {
    sha256: "7310ece418d634b04c28ff9adfb89bcf94d54be4d2badeccd49a8f08d1c4dcbc",
    width: 44,
    height: 44,
  },
  "Square71x71Logo.png": {
    sha256: "9f5830303de0ce688bc9558fa0d7e6fdc22a497015450d7ba0906a9e4e80f644",
    width: 71,
    height: 71,
  },
  "Square89x89Logo.png": {
    sha256: "0c8da9e37c03cdd952867abdf1f99e71d5074822a60e122e3bea9fcb63602809",
    width: 89,
    height: 89,
  },
  "Square107x107Logo.png": {
    sha256: "7e6af4ba375cc1a2da2ea9409621a6afd91c17e7e8f5468490f23ebdb72f23e7",
    width: 107,
    height: 107,
  },
  "Square142x142Logo.png": {
    sha256: "837bd157ccc5eaf3b5220846b8bf58d52ac4d872a883dab988a9cb2b2b05d05f",
    width: 142,
    height: 142,
  },
  "Square150x150Logo.png": {
    sha256: "d9cd2f52192558a6bdfbfe63d16998c5ede02c16045fb5ad3aea81b2cc16311d",
    width: 150,
    height: 150,
  },
  "Square284x284Logo.png": {
    sha256: "7b4131e9fddaad0a275e6c2b84d65607f34a71667f5d457980af1a0f623a96cb",
    width: 284,
    height: 284,
  },
  "Square310x310Logo.png": {
    sha256: "38c80a3aa3973f4d1172f45e74bd43304084616a0472c6416fb60efbb6ee9289",
    width: 310,
    height: 310,
  },
  "StoreLogo.png": {
    sha256: "2de93038eb04504976457f100856532f3a4faaf29f9aa827e08032315eb0e0ef",
    width: 50,
    height: 50,
  },
  "icon.png": {
    sha256: "df6ad8a4679ec98c5f9037d5a276e696d84e9cf4359682d96fdbf527b48e3101",
    width: 512,
    height: 512,
  },
};

const EXPECTED_PACKAGED_ICONS = {
  "icon.ico": "df7d62561601fcf00a0d08a5dfbcf7b360b1da0a65710ad9fff3ca879d39f40f",
  "icon.icns": "7ab33d03ba7b30e8e5181eaa440bc3b511daf944f1ae9413ce745f2dd7daa685",
} as const;

function sha256(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

function readIcon(path: string): Buffer {
  return readFileSync(resolve(iconDirectory, path));
}

describe("DiskSage product icon identity", () => {
  it("uses one reviewable vector source for the desktop and favicon identity", () => {
    const source = readFileSync(resolve(iconDirectory, "icon-source.svg"), "utf8");
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

  it("ships exact RGBA square PNGs for every configured desktop and store size", () => {
    const pngSignature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

    for (const [path, expected] of Object.entries(EXPECTED_PNGS)) {
      const bytes = readIcon(path);
      expect(bytes.subarray(0, 8)).toEqual(pngSignature);
      expect(bytes.readUInt32BE(16)).toBe(expected.width);
      expect(bytes.readUInt32BE(20)).toBe(expected.height);
      expect(bytes[24]).toBe(8);
      expect(bytes[25]).toBe(6);
      expect(sha256(bytes)).toBe(expected.sha256);
    }
  });

  it("ships a Windows ICO with the Tauri-recommended layer set and 32px first", () => {
    const bytes = readIcon("icon.ico");

    expect(sha256(bytes)).toBe(EXPECTED_PACKAGED_ICONS["icon.ico"]);
    expect(bytes.readUInt16LE(0)).toBe(0);
    expect(bytes.readUInt16LE(2)).toBe(1);
    expect(bytes.readUInt16LE(4)).toBe(6);

    const sizes: number[] = [];
    const bitDepths: number[] = [];
    for (let index = 0; index < 6; index += 1) {
      const offset = 6 + index * 16;
      const width = bytes[offset] === 0 ? 256 : bytes[offset];
      const height = bytes[offset + 1] === 0 ? 256 : bytes[offset + 1];
      sizes.push(width);
      expect(height).toBe(width);
      bitDepths.push(bytes.readUInt16LE(offset + 6));
    }

    expect(sizes).toEqual([32, 16, 24, 48, 64, 256]);
    expect(bitDepths).toEqual([32, 32, 32, 32, 32, 32]);
  });

  it("ships a structurally complete macOS ICNS container", () => {
    const bytes = readIcon("icon.icns");

    expect(sha256(bytes)).toBe(EXPECTED_PACKAGED_ICONS["icon.icns"]);
    expect(bytes.subarray(0, 4).toString("ascii")).toBe("icns");
    expect(bytes.readUInt32BE(4)).toBe(bytes.length);
    expect(bytes.subarray(8).includes(Buffer.from("ic10"))).toBe(true);
    expect(bytes.subarray(8).includes(Buffer.from("ic09"))).toBe(true);
    expect(bytes.subarray(8).includes(Buffer.from("ic08"))).toBe(true);
  });

  it("records the deterministic source and generated asset integrity contract", () => {
    const manifest = JSON.parse(
      readFileSync(resolve(iconDirectory, "icon-manifest.json"), "utf8"),
    ) as {
      schema: string;
      brand: string;
      source: string;
      source_sha256: string;
      assets: Array<{ path: string; sha256: string }>;
    };

    expect(manifest.schema).toBe("disksage.icon-set/v1");
    expect(manifest.brand).toBe("DiskSage");
    expect(manifest.source).toBe("icon-source.svg");
    expect(manifest.source_sha256).toBe(EXPECTED_SOURCE_SHA256);
    expect(Object.fromEntries(manifest.assets.map(({ path, sha256: digest }) => [path, digest]))).toEqual({
      ...Object.fromEntries(
        Object.entries(EXPECTED_PNGS).map(([path, expected]) => [path, expected.sha256]),
      ),
      ...EXPECTED_PACKAGED_ICONS,
    });
  });
});
