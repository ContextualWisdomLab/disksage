# DiskSage icon set

`icon-source.svg` is the reviewable source of truth for the DiskSage product identity. The same SVG is copied to `static/favicon.svg`; desktop and store PNGs, Windows ICO, and macOS ICNS are generated from that source.

The mark combines a disk platter, a verified-action check, and a small insight spark. It intentionally avoids the starter Svelte and Tauri marks so installed packages, browser surfaces, and operating-system launchers identify the actual product.

## Integrity contract

`icon-manifest.json` records the SHA-256 digest, dimensions, and color mode of every generated asset. `src/iconBrandingContract.test.ts` verifies that contract, including:

- identical canonical and favicon SVG content;
- square, 8-bit RGBA PNGs at every configured platform size;
- the Windows ICO layer order `32, 16, 24, 48, 64, 256`, all 32-bit;
- a structurally complete ICNS container;
- exact generated-asset digests.

An icon change is incomplete until the source, generated files, manifest, and contract test change together.

## Regeneration baseline

The checked-in set was rendered with CairoSVG 2.8.2 and Pillow 12.3.0, with ImageMagick 7.1.2-1 used to assemble the ordered ICO layers. Keep the source square and transparent-capable, then regenerate all tracked sizes and update the manifest only after visually checking 16 px, 32 px, 128 px, and 512 px output.

Tauri's icon guidance warns that its default icon set is not intended for shipped products and documents the required platform formats and ICO layer sizes. The bundle configuration continues to reference the generated PNG, ICO, and ICNS assets.

## Reference

Tauri Programme within The Commons Conservancy. (2025). *App icons*. Tauri. https://v2.tauri.app/develop/icons/
