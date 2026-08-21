import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { argv } from 'node:process';
import { fileURLToPath } from 'node:url';

/**
 * Read one JSON release manifest and return its non-empty version string.
 *
 * @param {string} manifestPath Repository-relative manifest path.
 * @param {(path: string, encoding: BufferEncoding) => string} readText Text reader seam.
 * @returns {string} The manifest version.
 */
export function readJsonVersion(manifestPath, readText = readFileSync) {
  let parsed;
  try {
    parsed = JSON.parse(readText(manifestPath, 'utf8'));
  } catch {
    throw new Error(`Release manifest ${manifestPath} is missing or invalid JSON.`);
  }
  if (
    parsed === null ||
    typeof parsed !== 'object' ||
    typeof parsed.version !== 'string' ||
    parsed.version.length === 0
  ) {
    throw new Error(
      `Release manifest ${manifestPath} must define one non-empty string version.`,
    );
  }
  return parsed.version;
}

/**
 * Read the Cargo package section and return its single literal version.
 *
 * Workspace-inherited or duplicated versions are refused because the packaged
 * application must expose one operator-verifiable version before publication.
 *
 * @param {string} manifestPath Repository-relative Cargo manifest path.
 * @param {(path: string, encoding: BufferEncoding) => string} readText Text reader seam.
 * @returns {string} The Cargo package version.
 */
export function readCargoPackageVersion(manifestPath, readText = readFileSync) {
  const lines = readText(manifestPath, 'utf8').split(/\r?\n/);
  let inPackage = false;
  const versions = [];
  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed === '[package]') {
      inPackage = true;
      continue;
    }
    if (inPackage && /^\[.*\]$/.test(trimmed)) break;
    if (!inPackage) continue;
    const match = line.match(/^\s*version\s*=\s*"([^"]+)"\s*(?:#.*)?$/);
    if (match) versions.push(match[1]);
  }
  if (versions.length !== 1) {
    throw new Error(
      `Release manifest ${manifestPath} must define exactly one package version.`,
    );
  }
  return versions[0];
}

/**
 * Validate manifest agreement, Semantic Versioning, and an optional release tag.
 *
 * @param {{packageVersion: string, cargoVersion: string, tauriVersion: string, githubRef?: string, githubRefName?: string}} input Version evidence.
 * @returns {string} Stable success message suitable for CI logs.
 */
export function validateReleaseVersion({
  packageVersion,
  cargoVersion,
  tauriVersion,
  githubRef = '',
  githubRefName = '',
}) {
  if (packageVersion !== cargoVersion || packageVersion !== tauriVersion) {
    throw new Error(
      `Release manifest versions disagree: package.json=${packageVersion}, Cargo.toml=${cargoVersion}, tauri.conf.json=${tauriVersion}.`,
    );
  }
  const semver =
    /^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-(?:(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?(?:\+(?:[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/;
  if (!semver.test(packageVersion)) {
    throw new Error(
      `Release manifest version ${packageVersion} is not valid Semantic Versioning.`,
    );
  }
  if (githubRef.startsWith('refs/tags/')) {
    const expectedTag = `v${packageVersion}`;
    if (githubRefName !== expectedTag) {
      throw new Error(
        `Release tag ${githubRefName} does not match manifest version ${expectedTag}.`,
      );
    }
  }
  return `Release version contract passed for ${packageVersion}.`;
}

/**
 * Read all release manifests from one repository root and validate their tag.
 *
 * @param {{repositoryRoot?: string, environment?: NodeJS.ProcessEnv, readText?: (path: string, encoding: BufferEncoding) => string}} options Runtime seams.
 * @returns {string} Stable success message.
 */
export function verifyReleaseVersion({
  repositoryRoot = process.cwd(),
  environment = process.env,
  readText = readFileSync,
} = {}) {
  return validateReleaseVersion({
    packageVersion: readJsonVersion(
      resolve(repositoryRoot, 'package.json'),
      readText,
    ),
    cargoVersion: readCargoPackageVersion(
      resolve(repositoryRoot, 'src-tauri/Cargo.toml'),
      readText,
    ),
    tauriVersion: readJsonVersion(
      resolve(repositoryRoot, 'src-tauri/tauri.conf.json'),
      readText,
    ),
    githubRef: environment.GITHUB_REF ?? '',
    githubRefName: environment.GITHUB_REF_NAME ?? '',
  });
}

/**
 * Run the release-version gate with injectable output and exit-code boundaries.
 *
 * @param {{verify?: () => string, writeOutput?: (message: string) => void, writeError?: (message: string) => void, setExitCode?: (code: number) => void}} options Runtime seams.
 * @returns {boolean} Whether validation passed.
 */
export function main({
  verify = verifyReleaseVersion,
  writeOutput = console.log,
  writeError = console.error,
  setExitCode = (code) => {
    process.exitCode = code;
  },
} = {}) {
  try {
    writeOutput(verify());
    return true;
  } catch (error) {
    writeError(error instanceof Error ? error.message : 'Unknown release version failure.');
    setExitCode(1);
    return false;
  }
}

if (argv[1] && resolve(argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
