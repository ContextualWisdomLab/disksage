import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

/** Read one checked-in file without copying production CLI ownership into the contract. */
function readRepositoryFile(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("cloud local-eviction batch CLI alias ownership", () => {
  it("gives the provider-neutral alias its own wrapper path while delegating to one canonical implementation", () => {
    const manifest = readRepositoryFile("src-tauri/Cargo.toml");
    const canonical = readRepositoryFile(
      "src-tauri/src/bin/disksage-icloud-local-eviction-batch.rs",
    );
    const wrapper = readRepositoryFile(
      "src-tauri/src/bin/disksage-cloud-local-eviction-batch.rs",
    );

    expect(manifest).toContain(
      'name = "disksage-icloud-local-eviction-batch"\npath = "src/bin/disksage-icloud-local-eviction-batch.rs"',
    );
    expect(manifest).toContain(
      'name = "disksage-cloud-local-eviction-batch"\npath = "src/bin/disksage-cloud-local-eviction-batch.rs"',
    );
    expect(canonical).toContain("pub(crate) fn main() {");
    expect(wrapper).toContain(
      '#[path = "disksage-icloud-local-eviction-batch.rs"]\nmod canonical_cli;',
    );
    expect(wrapper).toContain("canonical_cli::main();");
  });
});
