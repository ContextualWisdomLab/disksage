import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const generatorPath = resolve(repositoryRoot, "scripts/generate-icons.mjs");
const sourcePath = resolve(repositoryRoot, "src-tauri/icons/icon-source.svg");
const contractPath = resolve(repositoryRoot, "src-tauri/icons/icon-contract.json");
const PNG_SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

describe("Windows ICO compatibility", () => {
  it("uses DIB payloads below 256px and PNG only for the 256px layer", () => {
    const outputDirectory = mkdtempSync(resolve(tmpdir(), "disksage-windows-ico-"));
    try {
      execFileSync(
        process.execPath,
        [
          generatorPath,
          "--source",
          sourcePath,
          "--contract",
          contractPath,
          "--output",
          outputDirectory,
        ],
        { stdio: "pipe" },
      );

      const contract = JSON.parse(readFileSync(contractPath, "utf8")) as {
        ico: { bit_depth: number; path: string; sizes: number[] };
      };
      const bytes = readFileSync(resolve(outputDirectory, contract.ico.path));
      expect(bytes.readUInt16LE(0)).toBe(0);
      expect(bytes.readUInt16LE(2)).toBe(1);
      expect(bytes.readUInt16LE(4)).toBe(contract.ico.sizes.length);

      for (let index = 0; index < contract.ico.sizes.length; index += 1) {
        const size = contract.ico.sizes[index];
        const directoryOffset = 6 + index * 16;
        const payloadLength = bytes.readUInt32LE(directoryOffset + 8);
        const payloadOffset = bytes.readUInt32LE(directoryOffset + 12);
        const payload = bytes.subarray(payloadOffset, payloadOffset + payloadLength);

        if (size === 256) {
          expect(payload.subarray(0, PNG_SIGNATURE.length)).toEqual(PNG_SIGNATURE);
          continue;
        }

        expect(payload.readUInt32LE(0)).toBe(40);
        expect(payload.readInt32LE(4)).toBe(size);
        expect(payload.readInt32LE(8)).toBe(size * 2);
        expect(payload.readUInt16LE(12)).toBe(1);
        expect(payload.readUInt16LE(14)).toBe(contract.ico.bit_depth);
        expect(payload.readUInt32LE(16)).toBe(0);

        const xorBytes = size * size * 4;
        const andRowBytes = Math.ceil(size / 32) * 4;
        const andBytes = andRowBytes * size;
        expect(payload.readUInt32LE(20)).toBe(xorBytes + andBytes);
        expect(payload.length).toBe(40 + xorBytes + andBytes);
        expect(payload.subarray(0, PNG_SIGNATURE.length)).not.toEqual(PNG_SIGNATURE);
      }
    } finally {
      rmSync(outputDirectory, { force: true, recursive: true });
    }
  });
});
