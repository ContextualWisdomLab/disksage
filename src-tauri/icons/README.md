# DiskSage icon set

`icon-source.svg` is the reviewable source of truth for the DiskSage product identity. The same SVG is copied to `static/favicon.svg`. The mark combines a disk platter, a verified-action check, and a small insight spark so browser surfaces, installed packages, and operating-system launchers identify the actual product rather than the Svelte or Tauri starter templates.

## Generation boundary

`icon-contract.json` is the tracked cross-platform contract. It fixes the canonical source digest, every required square RGBA PNG size, the ordered Windows ICO layers, and the modern macOS ICNS chunks.

`scripts/generate-icons.mjs` uses only Node.js standard-library APIs. It rasterizes the mark with alpha-correct supersampling, writes metadata-free PNGs, assembles ICO and ICNS containers, and emits `icon-manifest.json` with the SHA-256 digest of every generated asset. Generated binary files and the generated manifest are ignored by Git because the source, contract, generator, and tests are the auditable inputs.

Tauri runs the generator before both development and production builds. The production bundle also includes `icon-manifest.json` as an integrity resource. Generation failure stops the Tauri command rather than falling back to a starter icon.

## Verification contract

`src/iconBrandingContract.test.ts` generates a fresh icon set in a temporary directory and verifies:

- identical canonical and favicon SVG content plus the exact source digest;
- the intended navy, platter, ring, verified-action, and insight colors at stable interior pixels;
- square, 8-bit RGBA PNGs at every contracted platform size;
- the Windows ICO layer order `32, 16, 24, 48, 64, 256`, all 32-bit;
- every contracted modern ICNS chunk and a valid container length;
- a manifest digest matching every generated file;
- fail-closed Tauri generation hooks for both development and package builds.

## Safe change procedure

1. Edit `icon-source.svg` and mirror the same content to `static/favicon.svg`.
2. Update `source_sha256` in `icon-contract.json`. If geometry or palette changes, update the corresponding generator constants and stable-pixel assertions.
3. Run `node scripts/generate-icons.mjs` and visually inspect at least 16 px, 32 px, 128 px, and 512 px.
4. Run `npm test`, `npm run build`, and a platform package build. Do not accept a package whose generated manifest is absent or whose icon contract fails.

## Reference

Tauri Programme within The Commons Conservancy. (2025). *App icons*. Tauri. https://v2.tauri.app/develop/icons/
