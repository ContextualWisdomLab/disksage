import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
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

function withFixture(mutate) {
  const root = mkdtempSync(join(tmpdir(), "verify-release-bundle-cardinality-"));
  try {
    buildValidArtifacts(root, runIdentity);
    mutate(root);
    return spawnSync("bash", [verifier, root, runIdentity], { encoding: "utf8" });
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

const bundleCases = [
  {
    label: "Debian bundle",
    directory: `release-disksage-ubuntu-22.04-${runIdentity}`,
    primary: "bundle/deb/disksage_1.0.0_amd64.deb",
    duplicate: "bundle/deb/disksage_1.0.0_arm64.deb",
  },
  {
    label: "AppImage bundle",
    directory: `release-disksage-ubuntu-22.04-${runIdentity}`,
    primary: "bundle/appimage/disksage_1.0.0_amd64.AppImage",
    duplicate: "bundle/appimage/disksage_1.0.0_arm64.AppImage",
  },
  {
    label: "Windows MSI bundle",
    directory: `release-disksage-windows-2022-${runIdentity}`,
    primary: "bundle/msi/disksage_1.0.0_x64_en-US.msi",
    duplicate: "bundle/msi/disksage_1.0.0_arm64_en-US.msi",
  },
  {
    label: "Windows NSIS bundle",
    directory: `release-disksage-windows-2022-${runIdentity}`,
    primary: "bundle/nsis/disksage_1.0.0_x64-setup.exe",
    duplicate: "bundle/nsis/disksage_1.0.0_arm64-setup.exe",
  },
  {
    label: "macOS DMG bundle",
    directory: `release-disksage-macos-latest-${runIdentity}`,
    primary: "bundle/dmg/disksage_1.0.0_aarch64.dmg",
    duplicate: "bundle/dmg/disksage_1.0.0_x86_64.dmg",
  },
];

for (const bundleCase of bundleCases) {
  test(`rejects zero or duplicate ${bundleCase.label}`, () => {
    assertRejected(
      withFixture((root) =>
        rmSync(join(root, bundleCase.directory, bundleCase.primary), { force: true }),
      ),
      `zero ${bundleCase.label}`,
    );

    assertRejected(
      withFixture((root) =>
        writeFixtureFile(
          join(root, bundleCase.directory, bundleCase.duplicate),
          `duplicate-${bundleCase.label}`,
        ),
      ),
      `two ${bundleCase.label}s`,
    );
  });
}
