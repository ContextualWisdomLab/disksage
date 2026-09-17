# DiskSage icon set

`icon-source.svg` is the reviewable and executable source of truth for the DiskSage product identity. The same SVG is mirrored to `static/favicon.svg`. The mark combines a disk platter, a verified-action check, and a small insight spark so browser surfaces, installed packages, and operating-system launchers identify the actual product rather than the Svelte or Tauri starter templates.

## Generation boundary

`icon-contract.json` is the tracked cross-platform contract. It binds the canonical SVG by SHA-256, fixes every required square RGBA PNG size, the ordered Windows ICO layers, the modern macOS ICNS chunks, and the supported icon-compressor ABI. Generation is supported only on Node.js major 20 with the exact zlib version recorded in that contract; a different compressor fails closed before any output directory or asset is created. `icon-manifest.json` records the exact Node.js and zlib versions that produced the files so an artifact digest cannot be presented without its compressor provenance.

`scripts/generate-icons.mjs` uses only Node.js standard-library APIs. It parses the canonical SVG's deliberately restricted `rect`, `circle`, filled `path`, and round-stroked `path` geometry, rasterizes those parsed shapes with alpha-correct supersampling, writes metadata-free PNGs, assembles ICO and ICNS containers, and emits `icon-manifest.json` with the SHA-256 digest of every generated asset. Unsupported or ambiguous SVG geometry, source/contract digest drift, or generator-runtime drift fails closed rather than silently falling back to a separately hard-coded mark or silently emitting byte-different assets. Generated binary files and the generated manifest are ignored by Git because the SVG source, contract, generator, and tests are the auditable inputs.

The package `predev` and `prebuild` lifecycle hooks generate resources while the canonical Tauri commands remain `npm run dev` and `npm run build`. Rust-only CI jobs that compile the Tauri crate generate the same resources explicitly before Cargo so Tauri resource validation observes the exact clean-checkout outputs. The production bundle includes `icon-manifest.json` as an integrity resource.

## Verification contract

`src/iconBrandingContract.test.ts`, `src/iconWindowsIcoCompatibility.test.ts`, and `src/iconGeneratorRuntimeContract.test.ts` generate fresh icon sets in temporary directories and verify:

- identical canonical and favicon SVG content plus the exact source digest;
- a controlled SVG palette mutation changes the corresponding native raster output, proving the SVG is the generation input rather than documentation-only metadata;
- the intended navy, platter, ring, verified-action, and insight colors at stable interior pixels;
- square, 8-bit RGBA PNGs at every contracted platform size;
- the Windows ICO layer order `32, 16, 24, 48, 64, 256`, with DIB payloads below 256 px and PNG only for the 256 px layer;
- every contracted modern ICNS chunk and a valid container length;
- a manifest digest matching every generated file plus the exact Node.js/zlib runtime that produced it;
- fail-closed generation before output when the contracted compressor ABI does not match the running process;
- canonical Tauri command values plus the npm lifecycle hooks that generate native resources.

## Safe change procedure

1. Use the contracted Node.js/zlib generator runtime. If the runtime contract itself must change, review that as a reproducibility change before regenerating assets.
2. Edit `icon-source.svg` using only the supported source subset and mirror the same content to `static/favicon.svg`.
3. Update `source_sha256` in `icon-contract.json`. The generator intentionally refuses to run while the SVG digest and contract disagree, so a source edit cannot silently produce assets under stale integrity metadata. If geometry or palette changes, update only stable-pixel assertions that are intentionally tied to the changed design; do not duplicate the geometry in the generator.
4. Run `node scripts/generate-icons.mjs` and visually inspect at least 16 px, 32 px, 128 px, and 512 px. Generation must fail if the SVG uses unsupported or ambiguous geometry.
5. Run `npm test`, `npm run build`, the Rust feature-build checks, and a platform package build. Do not accept a package whose generated manifest is absent or whose icon contract fails.

## Reference

Tauri Programme within The Commons Conservancy. (2025). *App icons*. Tauri. https://v2.tauri.app/develop/icons/
