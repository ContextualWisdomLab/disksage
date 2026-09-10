import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const verifier = fileURLToPath(
  new URL("../../.github/scripts/verify-release-artifacts.sh", import.meta.url),
);
const runIdentity = "12345";

const sha256 = (content) => createHash("sha256").update(content).digest("hex");
const checksumRecord = (name, content) => `${sha256(content)}  ${name}\n`;

function writeFixtureFile(path, content) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content);
}

// One valid release set: three platform directories, five bundle files, and
// twelve operational CLI files (six binaries plus six sidecars) = 17 files.
function buildValidArtifacts(root, runId) {
  const groups = {
    [`release-disksage-ubuntu-22.04-${runId}`]: {
      bundles: {
        "bundle/deb/disksage_1.0.0_amd64.deb": "deb-bundle",
        "bundle/appimage/disksage_1.0.0_amd64.AppImage": "appimage-bundle",
      },
      clis: [
        ["disksage-cloud-plan-linux-x86_64", "linux-cloud-plan"],
        ["disksage-duplicate-audit-linux-x86_64", "linux-duplicate-audit"],
      ],
    },
    [`release-disksage-windows-2022-${runId}`]: {
      bundles: {
        "bundle/msi/disksage_1.0.0_x64_en-US.msi": "msi-bundle",
        "bundle/nsis/disksage_1.0.0_x64-setup.exe": "nsis-bundle",
      },
      clis: [
        ["disksage-cloud-plan-windows-x86_64.exe", "windows-cloud-plan"],
        ["disksage-duplicate-audit-windows-x86_64.exe", "windows-duplicate-audit"],
      ],
    },
    [`release-disksage-macos-latest-${runId}`]: {
      bundles: {
        "bundle/dmg/disksage_1.0.0_aarch64.dmg": "dmg-bundle",
      },
      clis: [
        ["disksage-cloud-plan-macos-arm64", "macos-cloud-plan"],
        ["disksage-duplicate-audit-macos-arm64", "macos-duplicate-audit"],
      ],
    },
  };

  for (const [directory, spec] of Object.entries(groups)) {
    for (const [relative, content] of Object.entries(spec.bundles)) {
      writeFixtureFile(join(root, directory, relative), content);
    }
    for (const [name, content] of spec.clis) {
      writeFixtureFile(join(root, directory, name), content);
      writeFixtureFile(join(root, directory, `${name}.sha256`), checksumRecord(name, content));
    }
  }
}

function runVerifier(root, runId) {
  return spawnSync("bash", [verifier, root, runId], { encoding: "utf8" });
}

// Builds a valid set, applies `mutate`, runs the verifier, and always removes
// the temporary tree so no fixture leaks into the repository.
function withFixture(mutate) {
  const root = mkdtempSync(join(tmpdir(), "verify-release-artifacts-"));
  try {
    buildValidArtifacts(root, runIdentity);
    mutate?.(root);
    return runVerifier(root, runIdentity);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function withTempRoot(use) {
  const root = mkdtempSync(join(tmpdir(), "verify-release-artifacts-"));
  try {
    return use(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function assertRejected(result, label) {
  assert.notEqual(
    result.status,
    0,
    `${label} must be rejected; stdout/stderr: ${result.stdout}${result.stderr}`,
  );
}

test("accepts a complete, checksum-verified release artifact set", () => {
  const result = withFixture();
  assert.equal(
    result.status,
    0,
    `valid release set must pass; stdout/stderr: ${result.stdout}${result.stderr}`,
  );
});

test("rejects a missing or non-positive run identity", () => {
  withTempRoot((root) => {
    buildValidArtifacts(root, runIdentity);
    assertRejected(runVerifier(root, ""), "empty run identity");
    assertRejected(runVerifier(root, "0"), "zero run identity");
    assertRejected(runVerifier(root, "not-a-number"), "non-integer run identity");
  });
});

test("rejects a missing artifact root", () => {
  const missing = join(tmpdir(), `verify-release-artifacts-missing-${process.pid}`);
  rmSync(missing, { recursive: true, force: true });
  assertRejected(runVerifier(missing, runIdentity), "missing artifact root");
});

test("rejects an unexpected top-level entry", () => {
  assertRejected(
    withFixture((root) => writeFixtureFile(join(root, "stray.txt"), "stray")),
    "extra top-level entry",
  );
});

test("rejects a missing expected platform directory", () => {
  assertRejected(
    withFixture((root) =>
      rmSync(join(root, `release-disksage-macos-latest-${runIdentity}`), {
        recursive: true,
        force: true,
      }),
    ),
    "missing macOS directory",
  );
});

test("rejects an expected platform directory that is a symlink", () => {
  assertRejected(
    withFixture((root) => {
      const target = join(root, `release-disksage-macos-latest-${runIdentity}`);
      rmSync(target, { recursive: true, force: true });
      symlinkSync(join(root, `release-disksage-ubuntu-22.04-${runIdentity}`), target);
    }),
    "symlinked platform directory",
  );
});

test("rejects a missing or duplicated bundle file", () => {
  assertRejected(
    withFixture((root) =>
      rmSync(
        join(root, `release-disksage-ubuntu-22.04-${runIdentity}`, "bundle/deb/disksage_1.0.0_amd64.deb"),
      ),
    ),
    "zero Debian bundles",
  );
  assertRejected(
    withFixture((root) =>
      writeFixtureFile(
        join(root, `release-disksage-ubuntu-22.04-${runIdentity}`, "bundle/deb/disksage_1.0.0_arm64.deb"),
        "second-deb",
      ),
    ),
    "two Debian bundles",
  );
});

test("rejects a missing checksum sidecar", () => {
  assertRejected(
    withFixture((root) =>
      rmSync(join(root, `release-disksage-macos-latest-${runIdentity}`, "disksage-cloud-plan-macos-arm64.sha256")),
    ),
    "five checksum sidecars",
  );
});

test("rejects a checksum sidecar with more than one record", () => {
  assertRejected(
    withFixture((root) => {
      const directory = join(root, `release-disksage-macos-latest-${runIdentity}`);
      const name = "disksage-cloud-plan-macos-arm64";
      const digest = sha256(readFileSync(join(directory, name)));
      writeFileSync(join(directory, `${name}.sha256`), `${digest}  ${name}\n${digest}  ${name}\n`);
    }),
    "two-record checksum sidecar",
  );
});

test("rejects a checksum sidecar with a trailing field", () => {
  assertRejected(
    withFixture((root) => {
      const directory = join(root, `release-disksage-macos-latest-${runIdentity}`);
      const name = "disksage-duplicate-audit-macos-arm64";
      const digest = sha256(readFileSync(join(directory, name)));
      writeFileSync(join(directory, `${name}.sha256`), `${digest}  ${name}  trailing\n`);
    }),
    "checksum sidecar with trailing field",
  );
});

test("rejects a checksum sidecar naming a different asset", () => {
  assertRejected(
    withFixture((root) => {
      const directory = join(root, `release-disksage-ubuntu-22.04-${runIdentity}`);
      const name = "disksage-cloud-plan-linux-x86_64";
      const digest = sha256(readFileSync(join(directory, name)));
      writeFileSync(
        join(directory, `${name}.sha256`),
        `${digest}  disksage-duplicate-audit-linux-x86_64\n`,
      );
    }),
    "mismatched checksum asset name",
  );
});

test("rejects a checksum sidecar whose digest does not match the artifact", () => {
  assertRejected(
    withFixture((root) =>
      writeFileSync(
        join(root, `release-disksage-windows-2022-${runIdentity}`, "disksage-cloud-plan-windows-x86_64.exe"),
        "tampered",
      ),
    ),
    "digest mismatch",
  );
});

test("rejects an empty operational CLI", () => {
  assertRejected(
    withFixture((root) => {
      const directory = join(root, `release-disksage-macos-latest-${runIdentity}`);
      const name = "disksage-cloud-plan-macos-arm64";
      writeFileSync(join(directory, name), "");
      writeFileSync(join(directory, `${name}.sha256`), checksumRecord(name, ""));
    }),
    "empty operational CLI",
  );
});

test("rejects an unexpected regular file", () => {
  assertRejected(
    withFixture((root) =>
      writeFixtureFile(join(root, `release-disksage-ubuntu-22.04-${runIdentity}`, "unexpected-extra-file"), "extra"),
    ),
    "extra regular file",
  );
});

test("rejects a non-regular path inside the artifact tree", () => {
  assertRejected(
    withFixture((root) => {
      const directory = join(root, `release-disksage-ubuntu-22.04-${runIdentity}`);
      symlinkSync(join(directory, "disksage-cloud-plan-linux-x86_64"), join(directory, "linked-cli"));
    }),
    "non-regular artifact path",
  );
});
