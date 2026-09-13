import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const orphanPath = "src-tauri/src/orphan.rs";

/** Read the canonical orphan planner so macOS-only file-reading imports keep platform ownership. */
function readOrphanSource(): string {
  return readFileSync(resolve(repositoryRoot, orphanPath), "utf8");
}

describe("orphan planner macOS import ownership", () => {
  it("compiles File and Read only on macOS where bundle metadata consumes them", () => {
    const source = readOrphanSource();

    expect(source).toContain(
      '#[cfg(target_os = "macos")]\nuse std::fs::File;',
    );
    expect(source).toContain(
      '#[cfg(target_os = "macos")]\nuse std::io::Read;',
    );
    expect(source).toContain('#[cfg(target_os = "macos")]\nfn read_bundle_id(app: &Path) -> Option<String>');
    expect(source).toContain('File::open(plist_path)');
    expect(source).toContain('.read_to_end(&mut bytes)');
  });
});
